use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchConfig {
  pub game_name: String,
  pub mod_name: String,
  pub expected_hash: Option<String>,
  pub mod_file: String,
  pub bnr_file: Option<String>,

  pub output_name_iso: String,
  pub output_name_dol: String,
  /// This will override the output path for both ISO and DOL outputs
  pub output_path_override: Option<PathBuf>,
}