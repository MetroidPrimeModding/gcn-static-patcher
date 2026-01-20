pub struct Progress {
  pub current: u64,
  pub total: u64,
  pub description: Option<String>,
}

impl Default for Progress {
  fn default() -> Self {
    Progress {
      current: 0,
      total: 0,
      description: None,
    }
  }
}