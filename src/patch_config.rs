use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchConfig {
  pub game_name: String,
  pub mod_name: String,
  pub expected_hash: Option<String>,
}