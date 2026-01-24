#![cfg_attr(
  not(debug_assertions),
  windows_subsystem = "windows"
)] // hide console window on Windows in release

mod patch_iso;
mod patch_dol;
mod progress;
mod dol;
mod binser;
mod gcdisc;
mod patch_config;

use anyhow::Result;
use eframe;
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use log::{error, info};
use clap::Parser;
use crate::patch_config::PatchConfig;
use crate::patch_dol::patch_dol_file;
use crate::patch_iso::patch_iso_file;
use crate::progress::Progress;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
  /// Input file to patch (.dol or .iso)
  /// If provided, the app will run in CLI mode and exit after patching.
  #[arg(short, long, value_name = "FILE")]
  input_file: Option<PathBuf>,
  /// Output file path. If not provided, output will be next to input file.
  #[arg(short, long, value_name = "FILE")]
  output_file: Option<PathBuf>,
}

fn main() -> Result<()> {
  // Initialize logging
  env_logger::Builder::from_default_env()
    .target(env_logger::Target::Stdout)
    .filter_level(log::LevelFilter::Info)
    .init();

  let args = Args::parse();

  // TODO: load this from a file
  let patch_config = PatchConfig {
    game_name: "Metroid Prime 2: Echoes".to_string(),
    mod_name: "Echoes Practice Mod".to_string(),
    // expected_hash: Some("ce781ad1452311ca86667cf8dbd7d112".to_string()),
    expected_hash: None,
    mod_file: "prime-practice".to_string(),
    bnr_file: Some("python/opening_practice.bnr".to_string()),
    output_name_iso: "prime2-practice-mod.iso".to_string(),
    output_name_dol: "default_mod.dol".to_string(),
    output_path_override: args.output_file,
  };

  if let Some(input_path) = args.input_file {
   run_cli(&input_path, &patch_config)?;
  } else {
    info!("Running in GUI mode.");
    run_gui(patch_config)?;
  }

  Ok(())
}

fn run_cli(input_path: &PathBuf, patch_config: &PatchConfig) -> Result<()> {
  info!("Running in CLI mode. Input file: {:?}", input_path);
  let (progress_tx, progress_rx) = mpsc::channel();

  let result = handle_patch_for_file(
    input_path,
    patch_config,
    &progress_tx,
    // dummy context for CLI mode
    || {},
  );

  // Print progress updates
  while let Ok(progress) = progress_rx.recv() {
    if let Some(description) = &progress.description {
      println!("Progress: {:.2}% - {}", progress.ratio() * 100.0, description);
    } else {
      println!("Progress: {:.2}%", progress.ratio() * 100.0);
    }
  }

  match result {
    Ok(out_path) => {
      println!("Successfully patched file: {:?}", out_path);
      Ok(())
    }
    Err(e) => {
      eprintln!("Error patching file {:?}: {}", input_path, e);
      Err(e)
    }
  }
}

fn run_gui(patch_config: PatchConfig) -> Result<()> {
  let options = eframe::NativeOptions {
    viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 480.0]),
    ..Default::default()
  };
  eframe::run_native(
    "Patcher",
    options,
    Box::new(|cc| {
      egui_extras::install_image_loaders(&cc.egui_ctx);
      Ok(Box::<PatcherApp>::new(PatcherApp::new(patch_config)))
    }),
  ).map_err(|e| anyhow::anyhow!("Failed to start GUI: {}", e))
}

struct PatcherApp {
  config: PatchConfig,
  progress: Progress,
  progress_rx: Receiver<Progress>,
  progress_tx: Sender<Progress>,
}

impl PatcherApp {
  fn new(patch_config: PatchConfig) -> Self {
    let (progress_tx, progress_rx) = mpsc::channel();
    Self {
      config: patch_config,
      progress: Progress::new(0, 0, "Idle".to_string()),
      progress_rx,
      progress_tx,
    }
  }
}

impl eframe::App for PatcherApp {
  fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    while let Ok(progress) = self.progress_rx.try_recv() {
      self.progress = progress;
    }

    egui::CentralPanel::default().show(ctx, |ui| {
      ui.vertical_centered(|ui| {
        ui.heading(&self.config.game_name);
        ui.heading(&self.config.mod_name);
        ui.add_space(15.0);
        ui.label("Drag-and-drop a .dol or .iso to patch");
        ui.label("(or select with the button below)");
        ui.add_space(15.0);
        ui.label("The output file will be created next to the input file.");
        ui.add_space(15.0);
        if ui.button("Open file…").clicked() {
          if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.spawn_patch_thread(&path, ctx);
          }
        }
      });

      egui::TopBottomPanel::bottom("progress_bar").show_inside(ui, |ui| {
        ui.add_space(25.0);
        if let Some(description) = &self.progress.description {
          ui.vertical_centered(|ui| {
            if self.progress.error {
              // set font size larger for the warning icon
              ui.style_mut().override_text_style = Some(egui::TextStyle::Heading);
              ui.colored_label(egui::Color32::RED, "⚠ Error ⚠");
              ui.style_mut().override_text_style = None;
            }
            ui.label(description);
          });
        }
        let percentage = self.progress.ratio();
        ui.add(egui::ProgressBar::new(percentage).show_percentage());
      });

      preview_files_being_dropped(ui.ctx());

      // Collect dropped files:
      ui.input(|i| {
        let first_dropped_file = i.raw.dropped_files.first();
        if let Some(file) = first_dropped_file {
          if let Some(path) = &file.path {
            self.spawn_patch_thread(path, ctx);
          }
        }
      });
    });
  }
}

impl PatcherApp {
  fn spawn_patch_thread(&mut self, path: &PathBuf, ctx: &egui::Context) {
    info!("File dropped, spawning patch thread: {:?}", path);
    let path_clone = path.clone();
    let ctx_clone = ctx.clone();
    let progress_tx = self.progress_tx.clone();
    let config_clone = self.config.clone();
    // Spawn a new thread to handle the patching
    thread::spawn(move || {
      info!("Starting patch for file: {:?}", path_clone);
      let result = handle_patch_for_file(
        &path_clone,
        &config_clone,
        &progress_tx,
        || { ctx_clone.request_repaint(); },
      );
      match result {
        Ok(out_path) => {
          info!("Successfully patched file: {:?}", out_path);
          // "Done! <path>"
          let message = format!("Done! {:?}", out_path);
          progress_tx.send(Progress::new(1, 1, message)).ok();
          ctx_clone.request_repaint();
        }
        Err(e) => {
          error!("Error patching file {:?}: {} \n{}", path_clone, e, e.backtrace());
          let message = format!("{}", e);
          progress_tx.send(Progress::new_error(message)).ok();
          ctx_clone.request_repaint();
        }
      }
    });
  }
}

fn preview_files_being_dropped(ctx: &egui::Context) {
  use egui::{Align2, Color32, Id, LayerId, Order, TextStyle};
  use std::fmt::Write as _;

  if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
    let text = ctx.input(|i| {
      let mut text = "".to_owned();
      let first_dropped_file = i.raw.hovered_files.first();
      if let Some(file) = first_dropped_file {
        if let Some(path) = &file.path {
          write!(text, "\n{}", path.display()).ok();
        }
      }
      text
    });

    let painter =
      ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));

    let content_rect = ctx.content_rect();
    painter.rect_filled(content_rect, 0.0, Color32::from_black_alpha(192));
    painter.text(
      content_rect.center(),
      Align2::CENTER_CENTER,
      text,
      TextStyle::Heading.resolve(&ctx.style()),
      Color32::WHITE,
    );
  }
}

fn handle_patch_for_file<F>(
  path: &PathBuf,
  config: &PatchConfig,
  progress_tx: &Sender<Progress>,
  request_ui_update: F,
) -> Result<PathBuf> where F: Fn() {
  if let Some(ext) = path.extension() {
    if ext == "dol" {
      info!("Patching DOL file: {:?}", path);
      let out_path = config.output_path_override.clone()
        .unwrap_or_else(|| path.with_file_name(&config.output_name_dol));
      patch_dol_file(
        |new_progress| {
          let _ = progress_tx.send(new_progress);
          request_ui_update();
        },
        path,
        &out_path,
        &config,
      )?;
      Ok(out_path)
    } else if ext == "iso" || ext == "gcm" {
      info!("Patching ISO file: {:?}", path);
      let out_path = config.output_path_override.clone()
        .unwrap_or_else(|| path.with_file_name(&config.output_name_iso));
      patch_iso_file(
        |new_progress| {
          let _ = progress_tx.send(new_progress);
          request_ui_update();
        },
        path,
        &out_path,
        config,
      )?;
      Ok(out_path)
    } else {
      error!("Unsupported file type: {:?}", path);
      Err(anyhow::anyhow!("Unsupported file type: {:?}", ext))
    }
  } else {
    error!("No file extension found for: {:?}", path);
    Err(anyhow::anyhow!("No file extension found"))
  }
}
