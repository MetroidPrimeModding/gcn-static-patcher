mod patch_iso;
mod patch_dol;
mod progress;
mod dol;
mod binstream;
mod gcdisc;
mod patch_config;

pub use patch_config::{ModConfig, ModData};
pub use progress::Progress;

use anyhow::Result;
use clap::Parser;
use log::{error, info};
use object::{Object, ObjectSection};
use std::path::PathBuf;
use std::io::Read;
use std::fs;

use crate::patch_dol::patch_dol_file;
use crate::patch_iso::patch_iso_file;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
  /// Mod file path (.elf)
  /// If not provided, will look for "mod.elf" in the app directory.
  #[arg(short, long, value_name = "FILE", default_value = "mod.elf")]
  pub mod_file: PathBuf,

  /// Input file to patch (.dol or .iso)
  /// If provided, the app will run in CLI mode and exit after patching.
  #[arg(short, long, value_name = "FILE")]
  pub input_file: Option<PathBuf>,
  /// Output file path. If not provided, output will be next to input file.
  #[arg(short, long, value_name = "FILE")]
  pub output_file: Option<PathBuf>,
  /// Ignore hash check (may not work correctly)
  #[arg(long)]
  pub ignore_hash: bool,
}

pub fn load_mod_data(mod_path: PathBuf) -> Result<ModData> {
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
    let config: ModConfig = toml::from_str(config_str)
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
pub fn find_app_dir() -> PathBuf {
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

pub fn run_cli_mode(args: &Args, mut mod_data: ModData) -> Result<()> {
  let input_path = args.input_file.as_ref().ok_or_else(|| {
    anyhow::anyhow!("CLI mode requires an input file")
  })?;

  if args.ignore_hash {
    mod_data.config.expected_iso_hash = None;
    mod_data.config.expected_dol_hash = None;
  }
  if let Some(output_path) = &args.output_file {
    mod_data.config.output_path_override = Some(output_path.clone());
  }

  run_cli(input_path, &Some(mod_data))
}

pub fn run_cli(input_path: &PathBuf, patch_config: &Option<ModData>) -> Result<()> {
  info!("Running in CLI mode. Input file: {:?}", input_path);
  let result = handle_patch_for_file(
    input_path,
    patch_config,
    // dummy context for CLI mode
    |progress| {
      if let Some(description) = &progress.description {
        println!("Progress: {:.2}% - {}", progress.ratio() * 100.0, description);
      } else {
        println!("Progress: {:.2}%", progress.ratio() * 100.0);
      }
    },
  );

  match result {
    Ok(result) => {
      println!("Successfully patched file: {:?}", result);
      Ok(())
    }
    Err(e) => {
      eprintln!("Error patching file {:?}: {}", input_path, e);
      Err(e)
    }
  }
}

#[derive(Debug, Clone)]
pub enum PatchResult {
  Dol(PathBuf),
  Iso(PathBuf),
  ModData(ModData),
}

pub fn handle_patch_for_file<F>(
  path: &PathBuf,
  mod_data: &Option<ModData>,
  progres_fn: F,
) -> Result<PatchResult> where
  F: Fn(Progress),
{
  let ext = path.extension()
    .and_then(|s| s.to_str())
    .map(|s| s.to_lowercase());
  if ext == Some("dol".to_string()) {
    info!("Patching DOL file: {:?}", path);
    let Some(mod_data) = mod_data else {
      return Err(anyhow::anyhow!("No mod data loaded to patch DOL"));
    };
    let out_path = mod_data.config.output_path_override.clone()
      .unwrap_or_else(|| path.with_file_name(&mod_data.config.output_name_dol));
    patch_dol_file(
      progres_fn,
      path,
      &out_path,
      &mod_data,
    )?;
    Ok(PatchResult::Dol(out_path))
  } else if ext == Some("iso".to_string()) || ext == Some("gcm".to_string()) {
    let Some(mod_data) = mod_data else {
      return Err(anyhow::anyhow!("No mod data loaded to patch DOL"));
    };
    info!("Patching ISO file: {:?}", path);
    let out_path = mod_data.config.output_path_override.clone()
      .unwrap_or_else(|| path.with_file_name(&mod_data.config.output_name_iso));
    patch_iso_file(
      progres_fn,
      path,
      &out_path,
      mod_data,
    )?;
    Ok(PatchResult::Iso(out_path))
  } else {
    // check if it's an .elf
    const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
    // read the first 4 bytes of the file
    let mut magic = [0u8; 4];
    {
      let mut file = fs::File::open(path)?;
      file.read_exact(&mut magic)?;
    }
    if magic == ELF_MAGIC {
      let mod_data = load_mod_data(path.clone())?;
      info!("Loaded mod data from ELF: {:?}", path);
      return Ok(PatchResult::ModData(mod_data));
    }

    error!("Unsupported file type: {:?}", path);
    Err(anyhow::anyhow!("Unsupported file type: {:?}", ext))
  }
}
