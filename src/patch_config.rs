use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchConfig {
  pub game_name: String,
  pub mod_name: String,
  pub expected_hash: Option<String>,
  pub mod_file: String,
  pub bnr_file: Option<String>,
  pub output_name: String,
}