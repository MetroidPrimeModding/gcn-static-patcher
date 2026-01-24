use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ModData {
  pub elf_bytes: Vec<u8>,
  pub config: ModConfig,
  /// Whether to overwrite output files if they already exist
  pub overwrite_output: bool,
  /// This will override the output path for both ISO and DOL outputs
  /// Specified via CLI only
  pub output_path_override: Option<PathBuf>,
}

impl ModData {
  pub fn parse_elf(&self) -> Result<object::File<'_>, object::Error> {
    object::File::parse(&self.elf_bytes)
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModConfig {
  pub game_name: String,
  pub mod_name: String,
  pub version: String,
  pub expected_iso_hash: Option<String>,
  pub expected_dol_hash: Option<String>,
  pub bnr_file: Option<String>,

  pub output_name_iso: String,
  pub output_name_dol: String,

  /// Symbol name of the place to jump *from* to entry point of the mod
  pub entry_point_symbol: String,
  /// List of additional branch patches to apply
  #[serde(default)]
  pub branch_patches: Vec<PatchBranchConfig>,

}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchBranchConfig {
  pub branch_from_symbol: String,
  pub to_symbol: String,
}