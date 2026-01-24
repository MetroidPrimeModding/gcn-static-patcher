use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchConfig {
  pub game_name: String,
  pub mod_name: String,
  pub expected_iso_hash: Option<String>,
  pub expected_dol_hash: Option<String>,
  pub mod_file: String,
  pub bnr_file: Option<String>,

  pub output_name_iso: String,
  pub output_name_dol: String,

  /// Symbol name of the place to jump *from* to entry point of the mod
  pub entry_point_symbol: String,
  /// List of additional branch patches to apply
  #[serde(default)]
  pub branch_patches: Vec<PatchBranchConfig>,

  /// This will override the output path for both ISO and DOL outputs
  /// Specified via CLI only
  #[serde(skip, default)]
  pub output_path_override: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchBranchConfig {
  pub branch_from_symbol: String,
  pub to_symbol: String,
}