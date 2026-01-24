use std::fs;
use std::io::{Cursor, Seek, SeekFrom};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use log::info;
use md5::Digest;
use crate::binser::binstream::{BinStreamReadable, BinStreamWritable, BinStreamWrite};
use crate::dol::DolHeader;
use crate::gcdisc::{FSTEntry, GCDiscHeader, FST};
use crate::patch_config::PatchConfig;
use crate::patch_dol::patch_dol;
use crate::progress::Progress;

pub fn patch_iso_file<F>(
  progress_update: F,
  in_path: &PathBuf,
  out_path: &PathBuf,
  config: &PatchConfig,
) -> Result<()> where
  F: Fn(Progress),
{
  if out_path.exists() {
    return Err(anyhow::anyhow!("Output file already exists: {:?}", out_path));
  }

  info!("Preparing to patch ISO file...");
  let input_file = fs::File::open(in_path)?;
  let input_file_mmap = unsafe { memmap2::MmapOptions::new().map(&input_file)? };

  if let Some(expected_iso_hash) = config.expected_hash.clone() {
    info!("Verifying input ISO hash...");
    let mut hasher = md5::Md5::new();
    // Read the file in chunks to avoid high memory usage
    // update the progress bar as we go
    const CHUNK_SIZE: usize = 8 * 1024 * 1024;
    let mut processed_bytes = 0;
    let mut last_update = 0;
    let length = input_file_mmap.len();

    progress_update(Progress::new(0, length as u64, "Hashing ISO".to_string()));
    for chunk in input_file_mmap.chunks(CHUNK_SIZE) {
      hasher.update(chunk);
      processed_bytes += chunk.len();
      // only update ever 1MB to avoid spamming the UI
      if processed_bytes - last_update >= 1 * 1024 * 1024 {
        last_update = processed_bytes;
        progress_update(Progress::new(processed_bytes as u64, length as u64, "Hashing ISO".to_string()));
      }
    }
    progress_update(Progress::new(length as u64, length as u64, "Hashing ISO".to_string()));
    let result_hash = format!("{:x}", hasher.finalize());
    if result_hash != expected_iso_hash {
      return Err(anyhow::anyhow!(
                "Input ISO hash does not match expected hash. Expected: {}, Got: {}. Use ignore_hash option to bypass this check.",
                expected_iso_hash,
                result_hash
            ));
    }
    info!("Input ISO hash verified.");
  } else {
    info!("Skipping hash verification");
  }

  let mut input_reader = Cursor::new(&input_file_mmap[..]);
  let mut header = GCDiscHeader::read_from_stream(&mut input_reader)?;
  info!("Disk name: {}", header.name_string());

  input_reader.seek(SeekFrom::Start(header.fst_offset as u64))?;
  let mut fst = FST::read_from_stream(&mut input_reader)?;
  info!("FST contains {} entries", fst.root.count());

  // print("Removing Video/Attract02_32.thp to make room for mod")
  // attract = fst.find(["Video", "Attract02_32.thp"])
  // attract.length = 0
  info!("Removing Video/Attract02_32.thp to make room for mod");
  if let Some(FSTEntry::File { length, .. }) = fst.root.find_mut(&["<root>", "Video", "Attract02_32.thp"]) {
    *length = 0;
  } else {
    info!("Warning: Could not find Video/Attract02_32.thp in FST");
    info!("FST: {:?}", fst);
  }

  info!("Extracting dol...");
  let dol_header_bytes = &input_file_mmap[header.dol_offset as usize..(header.dol_offset + 0x100) as usize];
  let dol_header = DolHeader::read_from_stream(&mut Cursor::new(dol_header_bytes))?;
  let dol_length = dol_header.total_length();
  let unpatched_dol_bytes = &input_file_mmap[header.dol_offset as usize..(header.dol_offset + dol_length) as usize];

  info!("Patching dol...");
  let mod_path = std::env::current_dir()?
    .join(&config.mod_file);
  let mod_bytes = fs::read(mod_path)?;
  let patched_dol_bytes = patch_dol(&mod_bytes, unpatched_dol_bytes)?;

  info!("Finding a suitable gap...");
  let file_ranges = fst.root.get_ranges();
  let gaps = convert_ranges_to_gaps(&file_ranges);
  let search_size = patched_dol_bytes.len() as u32 + 8192; // extra padding
  let mut chosen_gap: Option<(u32, u32)> = None;
  for gap in gaps {
    let gap_size = gap.1 - gap.0;
    if gap_size >= patched_dol_bytes.len() as u32 {
      chosen_gap = Some(gap);
      break;
    }
  }
  if chosen_gap.is_none() {
    return Err(anyhow::anyhow!("Could not find a suitable gap in the ISO to fit the patched DOL"));
  }
  let chosen_gap = chosen_gap.unwrap();
  info!("Chosen gap: {:?}", chosen_gap);

  let mod_dol_offset = chosen_gap.0 - patched_dol_bytes.len() as u32;
  let mod_dol_offset = mod_dol_offset - (mod_dol_offset % 8192);
  info!("Mod DOL offset in ISO: 0x{:08X}", mod_dol_offset);

  info!("Patching FST...");
  fst.root.add_child(FSTEntry::File {
    name: "default_mod.dol".to_string(),
    offset: mod_dol_offset,
    length: patched_dol_bytes.len() as u32,
  })?;

  info!("Copying ISO...");
  let output_file = fs::File::options()
    .create(true).write(true).read(true)
    .open(out_path)?;
  output_file.set_len(input_file_mmap.len() as u64)?;
  let mut output_file_mmap = unsafe { memmap2::MmapOptions::new().map_mut(&output_file)? };
  // do it in chunks so we can update progress \
  {
    const CHUNK_SIZE: usize = 8 * 1024 * 1024;
    let mut processed_bytes = 0;
    let mut last_update = 0;
    let length = input_file_mmap.len();

    progress_update(Progress::new(0, length as u64, "Copying ISO".to_string()));
    for (in_chunk, out_chunk) in input_file_mmap.chunks(CHUNK_SIZE).zip(output_file_mmap.chunks_mut(CHUNK_SIZE)) {
      out_chunk.copy_from_slice(in_chunk);
      processed_bytes += in_chunk.len();
      // only update ever 1MB to avoid spamming the UI
      if processed_bytes - last_update >= 1 * 1024 * 1024 {
        last_update = processed_bytes;
        progress_update(Progress::new(processed_bytes as u64, length as u64, "Copying ISO".to_string()));
      }
    }
    progress_update(Progress::new(length as u64, length as u64, "Copying ISO".to_string()));
  }

  info!("Writing patched fst...");
  let fst_bytes = {
    let mut fst_bytes_vec = Vec::new();
    fst.write_to_stream(&mut Cursor::new(&mut fst_bytes_vec))?;
    fst_bytes_vec
  };
  let fst_offset = header.fst_offset as usize;
  let fst_size = fst_bytes.len();
  output_file_mmap[fst_offset..fst_offset + fst_size].copy_from_slice(&fst_bytes);

  info!("Patching header...");
  // write new string to the start of the game name
  Cursor::new(&mut header.game_name[..])
    .write_string(&config.game_name)?;
  header.dol_offset = mod_dol_offset;
  header.fst_offset = fst_offset as u32; // didn't actually move, but to be safe
  header.fst_size = fst_size as u32;
  header.fst_max_size = fst_size as u32;
  header.write_to_stream(&mut Cursor::new(&mut output_file_mmap[..]))?;

  info!("Writing patched dol...");
  let dol_offset = mod_dol_offset as usize;
  output_file_mmap[dol_offset..dol_offset + patched_dol_bytes.len()]
    .copy_from_slice(&patched_dol_bytes);

  if let Some(bnr_name) = &config.bnr_file {
    info!("Patching bnr...");
    let bnr_path = std::env::current_dir()?
      .join(bnr_name);
    let bnr_bytes = fs::read(bnr_path)?;
    let bnr_offset = header.user_pos as usize;
    output_file_mmap[bnr_offset..bnr_offset + bnr_bytes.len()]
      .copy_from_slice(&bnr_bytes);
  }

  info!("Closing files...");
  output_file_mmap.flush()?;

  progress_update(Progress::new(0, 0, "Done patching ISO".to_string()));
  Ok(())
}

fn convert_ranges_to_gaps(ranges: &Vec<(u32, u32)>) -> Vec<(u32, u32)> {
  let mut gaps = Vec::new();
  for i in 0..ranges.len() - 1 {
    let end_of_current = ranges[i].1;
    let start_of_next = ranges[i + 1].0;
    if start_of_next > end_of_current {
      gaps.push((end_of_current, start_of_next));
    }
  }
  gaps
}