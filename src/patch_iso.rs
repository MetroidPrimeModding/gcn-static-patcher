use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use crate::progress::Progress;

pub fn patch_iso_file(
    progress: Arc<Mutex<Progress>>,
    in_path: &PathBuf,
    out_path: &PathBuf,
    mod_path: &PathBuf,
    ignore_hash: bool,
) -> Result<()> {
    Ok(())
}