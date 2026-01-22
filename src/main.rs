#![cfg_attr(
  not(debug_assertions),
  windows_subsystem = "windows"
)] // hide console window on Windows in release

mod patch_iso;
mod patch_dol;
mod progress;
mod dol;
mod binser;

use anyhow::Result;
use eframe;
use eframe::egui;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use log::{error, info};
use crate::patch_dol::patch_dol_file;
use crate::patch_iso::patch_iso_file;
use crate::progress::Progress;

fn main() -> eframe::Result {
  env_logger::Builder::from_default_env()
    .target(env_logger::Target::Stdout)
    .filter_level(log::LevelFilter::Info)
    .init();

  let options = eframe::NativeOptions {
    viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
    ..Default::default()
  };
  eframe::run_native(
    "Patcher",
    options,
    Box::new(|cc| {
      egui_extras::install_image_loaders(&cc.egui_ctx);
      Ok(Box::<PatcherApp>::default())
    }),
  )?;

  Ok(())
}

struct PatcherApp {
  progress: Arc<Mutex<Progress>>,
}

impl Default for PatcherApp {
  fn default() -> Self {
    Self {
      progress: Arc::new(Mutex::new(Progress::default())),
    }
  }
}

impl eframe::App for PatcherApp {
  fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    egui::CentralPanel::default().show(ctx, |ui| {
      ui.vertical_centered(|ui| {
        ui.heading("Prime Practice Patcher");
        ui.add_space(15.0);
        ui.label("Drag-and-drop a default.dol or .iso to patch");
        ui.label("(or select with the button below)");
        ui.add_space(15.0);
        ui.label("The output file will be created next to the input file");
        ui.add_space(15.0);
        if ui.button("Open file…").clicked() {
          if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.spawn_patch_thread(&path);
          }
        }
      });

      preview_files_being_dropped(ui.ctx());

      // Collect dropped files:
      ui.input(|i| {
        let first_dropped_file = i.raw.dropped_files.first();
        if let Some(file) = first_dropped_file {
          if let Some(path) = &file.path {
            self.spawn_patch_thread(path);
          }
        }
      });
    });
  }
}

impl PatcherApp {
  fn spawn_patch_thread(&mut self, path: &PathBuf) {
    info!("File dropped, spawning patch thread: {:?}", path);
    let path_clone = path.clone();
    let progress_clone = Arc::clone(&self.progress);
    // Spawn a new thread to handle the patching
    thread::spawn(move || {
      info!("Starting patch for file: {:?}", path_clone);
      let result = handle_patch_for_file(&path_clone, progress_clone);
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

fn handle_patch_for_file(path: &PathBuf, progress: Arc<Mutex<Progress>>) -> Result<()> {
  if let Some(ext) = path.extension() {
    if ext == "dol" {
      info!("Patching DOL file: {:?}", path);
      let out_path = path.with_file_name("default_mod.dol");
      // mod is at cwd/prime-practice
      let mod_path = std::env::current_dir()?.join("prime-practice");
      patch_dol_file(
        progress,
        path,
        &out_path,
        &mod_path,
        false,
      )?;
    } else if ext == "iso" || ext == "gcm" {
      info!("Patching ISO file: {:?}", path);
      patch_iso_file(
        progress,
        path,
        &path.with_file_name("prime-practice-mod.iso"),
        &std::env::current_dir()?.join("prime-practice"),
        false,
      )?;
    } else {
      error!("Unsupported file type: {:?}", path);
    }
  } else {
    error!("No file extension found for: {:?}", path);
  }
  Ok(())
}