#![cfg_attr(
  not(debug_assertions),
  windows_subsystem = "windows"
)] // hide console window on Windows in release

mod patch_iso;
mod patch_dol;
mod progress;
mod dol;
mod binstream;
mod gcdisc;
mod patch_config;

use crate::patch_config::{ModConfig, ModData};
use crate::patch_dol::patch_dol_file;
use crate::patch_iso::patch_iso_file;
use crate::progress::Progress;
use anyhow::Result;
use clap::Parser;
use eframe;
use eframe::egui;
use log::{error, info};
use object::{Object, ObjectSection};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::{fs, thread};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
  /// Mod file path (.elf)
  /// If not provided, will look for "mod.elf" in the app directory.
  #[arg(short, long, value_name = "FILE", default_value = "mod.elf")]
  mod_file: PathBuf,

  /// Input file to patch (.dol or .iso)
  /// If provided, the app will run in CLI mode and exit after patching.
  #[arg(short, long, value_name = "FILE")]
  input_file: Option<PathBuf>,
  /// Output file path. If not provided, output will be next to input file.
  #[arg(short, long, value_name = "FILE")]
  output_file: Option<PathBuf>,
  /// Ignore hash check (may not work correctly)
  #[arg(long)]
  ignore_hash: bool,
}

fn main() -> Result<()> {
  // Initialize logging
  env_logger::Builder::from_default_env()
    .target(env_logger::Target::Stdout)
    .filter_level(log::LevelFilter::Info)
    .init();

  let args = Args::parse();

  // load from patcher_config.toml
  let mut patch_config: ModData = load_patch_data(&args)?;

  // Override output path if provided via CLI
  if let Some(output_path) = &args.output_file {
    patch_config.config.output_path_override = Some(output_path.clone());
  }

  if let Some(input_path) = args.input_file {
    if args.ignore_hash {
      patch_config.config.expected_iso_hash = None;
      patch_config.config.expected_dol_hash = None;
    }
    run_cli(&input_path, &patch_config)?;
  } else {
    run_gui(&args, patch_config)?;
  }

  Ok(())
}

fn load_patch_data(args: &Args) -> Result<ModData> {
  let mut mod_path = std::env::current_dir()?
    .join(&args.mod_file);

  if !mod_path.exists() {
    let app_dir = find_app_dir();
    mod_path = app_dir.join(&args.mod_file);
  }

  if !mod_path.exists() {
    return Err(anyhow::anyhow!("Mod file not found: {:?}", mod_path));
  }

  // if the mod .elf exists, load from the section ".patcher_config" inside the ELF
  info!("Loading patcher config from ELF section");
  let elf_bytes = fs::read(&mod_path)
    .map_err(|e| anyhow::anyhow!("Failed to read mod ELF file: {}", e))?;
  let elf_file = object::File::parse(&*elf_bytes)
    .map_err(|e| anyhow::anyhow!("Failed to parse mod ELF file: {}", e))?;
  if let Some(section) = elf_file.section_by_name(".patcher_config") {
    // this is a PT_NOTE section containing the TOML config


    let patcher_config_section = section.data()
      .map_err(|e| anyhow::anyhow!("Failed to read .patcher_config section data: {}", e))?;
    info!("deb: {:?}", section.kind());

    let config_str = std::str::from_utf8(patcher_config_section)
      .map_err(|e| anyhow::anyhow!("Failed to parse .patcher_config section as UTF-8: {}", e))?;
    let config = toml::from_str(config_str)
      .map_err(|e| anyhow::anyhow!("Failed to parse patcher config from ELF section: {}", e))?;

    Ok(ModData {
      elf_bytes,
      config,
    })
  } else {
    Err(anyhow::anyhow!(".patcher_config section not found in mod ELF"))
  }
}

/// Returns the directory containing the executable. On macOS bundles, this will return the directory containing the .app bundle.
fn find_app_dir() -> PathBuf {
  #[cfg(target_os = "macos")]
  {
    use std::path::Path;
    let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let mut dir = exe_path.parent();
    while let Some(d) = dir {
      if d.file_name().and_then(|n| n.to_str()) == Some("MacOS") {
        if let Some(app_dir) = d.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
          return app_dir.to_path_buf();
        }
      }
      dir = d.parent();
    }
    exe_path.parent().unwrap_or(Path::new(".")).to_path_buf()
  }
  #[cfg(not(target_os = "macos"))]
  {
    std::env::current_exe()
      .ok()
      .and_then(|p| p.parent().map(|d| d.to_path_buf()))
      .unwrap_or_else(|| PathBuf::from("."))
  }
}

fn run_cli(input_path: &PathBuf, patch_config: &ModData) -> Result<()> {
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

fn run_gui(args: &Args, patch_config: ModData) -> Result<()> {
  info!("Running in GUI mode.");
  let options = eframe::NativeOptions {
    viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 480.0]),
    ..Default::default()
  };
  eframe::run_native(
    "Patcher",
    options,
    Box::new(|cc| {
      egui_extras::install_image_loaders(&cc.egui_ctx);
      let mut app = Box::<PatcherApp>::new(PatcherApp::new(patch_config));
      if args.ignore_hash {
        app.ignore_hash = true;
      }
      Ok(app)
    }),
  ).map_err(|e| anyhow::anyhow!("Failed to start GUI: {}", e))
}

struct PatcherApp {
  // TODO: Option<>
  mod_data: ModData,
  progress: Progress,
  progress_rx: Receiver<Progress>,
  progress_tx: Sender<Progress>,
  ignore_hash: bool,
}

impl PatcherApp {
  fn new(mod_data: ModData) -> Self {
    let (progress_tx, progress_rx) = mpsc::channel();
    Self {
      mod_data,
      progress: Progress::new(0, 0, "Idle".to_string()),
      progress_rx,
      progress_tx,
      ignore_hash: false,
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
        ui.heading(&self.mod_data.config.game_name);
        ui.heading(&self.mod_data.config.mod_name);
        ui.add_space(15.0);
        ui.label("Drag-and-drop a .dol or .iso to patch");
        ui.label("(or select with the button below)");
        ui.add_space(15.0);
        ui.label("The output file will be created next to the input file.");
        ui.add_space(15.0);
        ui.checkbox(&mut self.ignore_hash, "Ignore hash check");
        if self.ignore_hash {
          ui.colored_label(egui::Color32::from_rgb(200, 20, 20), "Warning: Modified inputs may cause the patch to fail or the game to crash");
        }
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
    let mut mod_data_clone = self.mod_data.clone();
    if self.ignore_hash {
      mod_data_clone.config.expected_iso_hash = None;
      mod_data_clone.config.expected_dol_hash = None;
    }
    // Spawn a new thread to handle the patching
    thread::spawn(move || {
      info!("Starting patch for file: {:?}", path_clone);
      let result = handle_patch_for_file(
        &path_clone,
        &mod_data_clone,
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
  mod_data: &ModData,
  progress_tx: &Sender<Progress>,
  request_ui_update: F,
) -> Result<PathBuf> where
  F: Fn(),
{
  if let Some(ext) = path.extension() {
    if ext == "dol" {
      info!("Patching DOL file: {:?}", path);
      let out_path = mod_data.config.output_path_override.clone()
        .unwrap_or_else(|| path.with_file_name(&mod_data.config.output_name_dol));
      patch_dol_file(
        |new_progress| {
          let _ = progress_tx.send(new_progress);
          request_ui_update();
        },
        path,
        &out_path,
        &mod_data,
      )?;
      Ok(out_path)
    } else if ext == "iso" || ext == "gcm" {
      info!("Patching ISO file: {:?}", path);
      let out_path = mod_data.config.output_path_override.clone()
        .unwrap_or_else(|| path.with_file_name(&mod_data.config.output_name_iso));
      patch_iso_file(
        |new_progress| {
          let _ = progress_tx.send(new_progress);
          request_ui_update();
        },
        path,
        &out_path,
        mod_data,
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
