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
use crate::patch_config::PatchConfig;
use crate::patch_dol::patch_dol_file;
use crate::patch_iso::patch_iso_file;
use crate::progress::Progress;

fn main() -> eframe::Result {
  env_logger::Builder::from_default_env()
    .target(env_logger::Target::Stdout)
    .filter_level(log::LevelFilter::Info)
    .init();

  // TODO: load this from a file
  let patch_config = PatchConfig {
    game_name: "Metroid Prime 2: Echoes".to_string(),
    mod_name: "Echoes Practice Mod".to_string(),
    // expected_hash: Some("ce781ad1452311ca86667cf8dbd7d112".to_string()),
    expected_hash: None,
    output_name: "prime2-practice-mod.iso".to_string(),
    mod_file: "prime-practice".to_string(),
    bnr_file: Some("python/opening_practice.bnr".to_string()),
  };

  let options = eframe::NativeOptions {
    viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
    ..Default::default()
  };
  eframe::run_native(
    "Patcher",
    options,
    Box::new(|cc| {
      egui_extras::install_image_loaders(&cc.egui_ctx);
      Ok(Box::<PatcherApp>::new(PatcherApp::new(patch_config)))
    }),
  )?;

  Ok(())
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
        ui.label("The output file will be created next to the input file");
        ui.add_space(15.0);
        if ui.button("Open file…").clicked() {
          if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.spawn_patch_thread(&path, ctx);
          }
        }
        // progress bar
        ui.add_space(25.0);
        if let Some(description) = &self.progress.description {
          ui.label(description);
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
        progress_tx,
        &ctx_clone,
      );
      // TODO: when done, send back to the UI.
      // If an error, it might be "ignore hash?"
      match result {
        Ok(_) => info!("Successfully patched file: {:?}", path_clone),
        Err(e) => error!("Error patching file {:?}: {} \n{}", path_clone, e, e.backtrace()),
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

fn handle_patch_for_file(
  path: &PathBuf,
  config: &PatchConfig,
  progress_tx: Sender<Progress>,
  ctx: &egui::Context,
) -> Result<()> {
  if let Some(ext) = path.extension() {
    if ext == "dol" {
      info!("Patching DOL file: {:?}", path);
      let out_path = path.with_file_name(format!(
        "{}_mod.dol",
        path.file_stem()
          .and_then(|s| s.to_str())
          .unwrap_or("output")
      ));
      // mod is at cwd/prime-practice
      let mod_path = std::env::current_dir()?.join("prime-practice");
      patch_dol_file(
        |new_progress| {
          let _ = progress_tx.send(new_progress);
          ctx.request_repaint();
        },
        path,
        &out_path,
        &config,
      )?;
    } else if ext == "iso" || ext == "gcm" {
      info!("Patching ISO file: {:?}", path);
      patch_iso_file(
        |new_progress| {
          let _ = progress_tx.send(new_progress);
          ctx.request_repaint();
        },
        path,
        &path.with_file_name(&config.output_name),
        config,
      )?;
    } else {
      error!("Unsupported file type: {:?}", path);
    }
  } else {
    error!("No file extension found for: {:?}", path);
  }
  Ok(())
}
