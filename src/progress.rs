#[derive(Debug, Clone)]
pub struct Progress {
  pub current: u64,
  pub total: u64,
  pub description: Option<String>,
  pub error: bool,
}

impl Progress {
  pub fn new(current: u64, total: u64, description: String) -> Self {
    Progress {
      current,
      total,
      description: Some(description),
      error: false,
    }
  }

  pub fn new_error(description: String) -> Self {
    Progress {
      current: 0,
      total: 0,
      description: Some(description),
      error: true,
    }
  }

  pub fn ratio(&self) -> f32 {
    if self.total == 0 {
      1.0 // total is zero, consider it complete
    } else {
      (self.current as f64 / self.total as f64) as f32
    }
  }
}

impl Default for Progress {
  fn default() -> Self {
    Progress {
      current: 0,
      total: 0,
      description: None,
      error: false,
    }
  }
}