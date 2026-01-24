#![cfg_attr(
  not(debug_assertions),
  windows_subsystem = "windows"
)] // hide console window on Windows in release

use anyhow::Result;
use clap::Parser;
use eframe;
use eframe::egui;
use log::{error, info};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use gcn_static_patcher::{
  Args,
  ModData,
  PatchResult,
  Progress,
  find_app_dir,
  handle_patch_for_file,
  load_mod_data,
  run_cli_mode,
};

fn main() -> Result<()> {
  // Initialize logging
  let log_file_path = find_app_dir().join("patcher.log");
  println!("Log file path: {:?}", log_file_path);
  fern::Dispatch::new()
    .format(|out, message, record| {
      out.finish(format_args!(
        "{}[{}][{}] {}",
        chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
        record.target(),
        record.level(),
        message
      ))
    })
    .level(log::LevelFilter::Info)
    .chain(std::io::stdout())
    .chain(fern::log_file(log_file_path)?)
    .apply()?;

  let args = Args::parse();

  let mut mod_path = std::env::current_dir()?
    .join(&args.mod_file);

  if !mod_path.exists() {
    let app_dir = find_app_dir();
    mod_path = app_dir.join(&args.mod_file);
  }
  let mod_data = load_mod_data(mod_path);

  if args.input_file.is_some() {
    let mod_data = mod_data?;
    run_cli_mode(&args, mod_data)?;
  } else {
    let mod_data = mod_data.ok();
    run_gui(args, mod_data)?;
  }

  Ok(())
}

fn run_gui(args: Args, mod_data: Option<ModData>) -> Result<()> {
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
      Ok(Box::new(PatcherApp::new(args, mod_data)))
    }),
  ).map_err(|e| anyhow::anyhow!("Failed to start GUI: {}", e))
}

struct PatcherApp {
  // args: Args,
  mod_data: Option<ModData>,
  progress: Progress,
  progress_rx: Receiver<Progress>,
  progress_tx: Sender<Progress>,
  mod_data_rx: Receiver<ModData>,
  mod_data_tx: Sender<ModData>,
  ignore_hash: bool,
  overwrite_output: bool,
}

impl PatcherApp {
  fn new(args: Args, mod_data: Option<ModData>) -> Self {
    let (progress_tx, progress_rx) = mpsc::channel();
    let (mod_data_tx, mod_data_rx) = mpsc::channel();
    let ignore_hash = args.ignore_hash;
    let overwrite_output = args.overwrite;
    Self {
      mod_data,
      progress: Progress::new(0, 0, "Idle".to_string()),
      progress_rx,
      progress_tx,
      mod_data_rx,
      mod_data_tx,
      ignore_hash,
      overwrite_output,
    }
  }
}

impl eframe::App for PatcherApp {
  fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    while let Ok(mod_data) = self.mod_data_rx.try_recv() {
      self.mod_data = Some(mod_data);
    }

    while let Ok(progress) = self.progress_rx.try_recv() {
      self.progress = progress;
    }

    egui::CentralPanel::default().show(ctx, |ui| {
      ui.vertical_centered(|ui| {
        ui.heading("GCN Static Patcher");
        ui.add_space(10.0);
      });
      if self.mod_data.is_some() {
        ui.vertical_centered(|ui| {
          let mod_data = self.mod_data.as_ref().unwrap();
          ui.heading(&mod_data.config.game_name);
          ui.heading(format!("{} v{}", &mod_data.config.mod_name, &mod_data.config.version));
          ui.add_space(15.0);
          ui.label("Drag-and-drop a .dol or .iso to patch");
          ui.label("(or select with the button below)");
          ui.add_space(15.0);
          ui.label("The output file will be created next to the input file.");
          ui.add_space(15.0);
          ui.checkbox(&mut self.overwrite_output, "Overwrite existing");
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
      } else {
        ui.vertical_centered(|ui| {
          ui.heading("No mod loaded");
          ui.add_space(15.0);
          ui.label("Drag-and-drop a mod file");
          ui.label("(or select with the button below)");
          ui.add_space(15.0);
          if ui.button("Open file…").clicked() {
            if let Some(path) = rfd::FileDialog::new().pick_file() {
              self.spawn_patch_thread(&path, ctx);
            }
          }
        });
      }

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
    let mut mod_data_clone = self.mod_data.clone();
    if let Some(mod_data_clone) = &mut mod_data_clone {
      if self.ignore_hash {
        mod_data_clone.config.expected_iso_hash = None;
        mod_data_clone.config.expected_dol_hash = None;
      }
      mod_data_clone.overwrite_output = self.overwrite_output;
    }

    // Spawn a new thread to handle the patching
    let ctx_clone = ctx.clone();
    let path_clone = path.clone();
    let progress_tx = self.progress_tx.clone();
    let mod_data_tx = self.mod_data_tx.clone();
    thread::spawn(move || {
      info!("Starting patch for file: {:?}", path_clone);
      let result = handle_patch_for_file(
        &path_clone,
       &mod_data_clone,
        |progress| {
          let _ = progress_tx.send(progress);
          ctx_clone.request_repaint();
        },
      );
      match result {
        Ok(out_path) => {
          match out_path {
            PatchResult::Dol(path) | PatchResult::Iso(path) => {
              info!("Patched DOL file created at: {:?}", path);
              let message = format!("Done! {:?}", path);
              progress_tx.send(Progress::new(1, 1, message)).ok();
              ctx_clone.request_repaint();
            }
            PatchResult::ModData(mod_data) => {
              mod_data_tx.send(mod_data).ok();
              info!("Loaded mod data from ELF: {:?}", path_clone);
              ctx_clone.request_repaint();
            }
          }
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
