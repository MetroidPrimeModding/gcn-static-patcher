use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use log::info;
use md5::Digest;
use crate::progress::Progress;

// mp2 0-00 hash of a clean ISO (md5)
const expected_iso_hash: &str = "ce781ad1452311ca86667cf8dbd7d112";

pub fn patch_iso_file<F>(
    progress_update: F,
    in_path: &PathBuf,
    out_path: &PathBuf,
    mod_path: &PathBuf,
    ignore_hash: bool,
) -> Result<()> where F: Fn(Progress) {
    info!("Preparing to patch ISO file...");
    let input_file = std::fs::File::open(in_path)?;
    let input_file_mmap = unsafe { memmap2::MmapOptions::new().map(&input_file)? };

    if ignore_hash {
        info!("Skipping hash verification");
    } else {
        info!("Verifying input ISO hash...");
        let mut hasher = md5::Md5::new();
        // Read the file in chunks to avoid high memory usage
        // update the progress bar as we go
        const CHUNK_SIZE: usize = 8 * 1024 * 1024;
        let mut processed_bytes = 0;
        let length = input_file_mmap.len();

        progress_update(Progress::new(0, length as u64, "Hashing ISO".to_string()));

        for chunk in input_file_mmap.chunks(CHUNK_SIZE) {
            hasher.update(chunk);
            processed_bytes += chunk.len();
            progress_update(Progress::new(processed_bytes as u64, length as u64, "Hashing ISO".to_string()));
        }
        let result_hash = format!("{:x}", hasher.finalize());
        if result_hash != expected_iso_hash {
            return Err(anyhow::anyhow!(
                "Input ISO hash does not match expected hash. Expected: {}, Got: {}. Use ignore_hash option to bypass this check.",
                expected_iso_hash,
                result_hash
            ));
        }
        info!("Input ISO hash verified.");
    }

    // let output_file = std::fs::File::create(out_path)?;
    // let mut output_file_mmap = unsafe { memmap2::MmapOptions::new().map_mut(&output_file)? };



    Ok(())
}