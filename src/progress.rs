#[derive(Debug, Clone)]
pub struct Progress {
  pub current: u64,
  pub total: u64,
  pub description: Option<String>,
}

impl Progress {
  pub fn new(current: u64, total: u64, description: String) -> Self {
    Progress {
      current,
      total,
      description: Some(description),
    }
  }

  pub fn percentage(&self) -> f32 {
    if self.total == 0 {
      0.0
    } else {
      ((self.current as f64 / self.total as f64) * 100.0) as f32
    }
  }
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