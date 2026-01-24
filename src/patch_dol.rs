use crate::binstream::{BinStreamRead, BinStreamReadable, BinStreamWritable, BinStreamWrite};
use crate::dol::DolHeader;
use crate::patch_config::{ModConfig, ModData};
use crate::progress::Progress;
use anyhow::Result;
use log::info;
use md5::Digest;
use object::{Object, ObjectSection, ObjectSegment, ObjectSymbol};
use std::fs;
use std::io;
use std::path::PathBuf;

pub fn patch_dol_file<F>(
  progress_update: F,
  in_path: &PathBuf,
  out_path: &PathBuf,
  mod_data: &ModData,
) -> Result<()> where F: Fn(Progress) {
  if out_path.exists() {
    return Err(anyhow::anyhow!("Output file already exists: {:?}", out_path));
  }

  progress_update(Progress::new(0, 4, "Reading DOL".to_string()));
  info!("Preparing to patch DOL file...");
  info!("Reading DOL file from {:?}", in_path);
  let dol_bytes = fs::read(in_path)?;
  info!("Read DOL file: {} bytes", dol_bytes.len());

  progress_update(Progress::new(1, 4, "Patching DOL".to_string()));
  // path is relative to the executable
  let out_bytes = patch_dol(&mod_data, &dol_bytes)?;

  progress_update(Progress::new(3, 4, "Writing DOL".to_string()));
  info!("Writing patched DOL file to {:?}", out_path);
  fs::write(out_path, &out_bytes)?;
  info!("Len of patched DOL file: {} bytes", out_bytes.len());
  info!("Mod size (in dol): {} bytes", out_bytes.len() - dol_bytes.len());
  progress_update(Progress::new(4, 4, "Done patching dol".to_string()));

  Ok(())
}

pub fn patch_dol(
  mod_data: &ModData,
  dol_bytes: &[u8],
) -> Result<Vec<u8>> {
  if let Some(expected_dol_hash) = mod_data.config.expected_dol_hash.clone() {
    info!("Verifying input DOL hash...");
    let mut hasher = md5::Md5::new();
    hasher.update(dol_bytes);
    let result_hash = format!("{:x}", hasher.finalize());
    if result_hash != expected_dol_hash {
      return Err(anyhow::anyhow!(
                "Input DOL hash does not match expected hash. Expected: {}, Got: {}. Check \"Ignore Hash\" option to bypass this check.",
                expected_dol_hash,
                result_hash
            ));
    }
  }

  let mut dol_header = DolHeader::read_from_stream(&mut io::Cursor::new(dol_bytes))?;
  info!("DOL Header: {:?}", dol_header);

  let mod_file = mod_data.parse_elf()?;

  let symbol_map = mod_file.symbols()
    .filter_map(|sym| {
      if let Ok(name) = sym.name() {
        Some((name.to_string(), sym))
      } else {
        None
      }
    })
    .collect::<std::collections::HashMap<_, _>>();

  let entry_addr = mod_file.entry();

  // let link_start = symbol_map.get("_LINK_START")
  //   .ok_or_else(|| anyhow::anyhow!("Missing symbol _LINK_START"))?
  //   .address();
  let link_end = symbol_map.get("_LINK_END")
    .ok_or_else(|| anyhow::anyhow!("Missing symbol _LINK_END"))?
    .address();
  // let link_size = symbol_map.get("_LINK_SIZE")
  //   .ok_or_else(|| anyhow::anyhow!("Missing symbol _LINK_SIZE"))?
  //   .address();
  let patch_arena_lo_1 = symbol_map.get("_PATCH_ARENA_LO_1")
    .ok_or_else(|| anyhow::anyhow!("Missing symbol _PATCH_ARENA_LO_1"))?
    .address();
  let patch_arena_lo_2 = symbol_map.get("_PATCH_ARENA_LO_2")
    .ok_or_else(|| anyhow::anyhow!("Missing symbol _PATCH_ARENA_LO_2"))?
    .address();
  let patch_earlyboot_memset = symbol_map.get("_PATCH_EARLYBOOT_MEMSET")
    .ok_or_else(|| anyhow::anyhow!("Missing symbol _PATCH_EARLYBOOT_MEMSET"))?
    .address();
  let patch_earlyboot_memset_addr = symbol_map.get("_earlyboot_memset")
    .ok_or_else(|| anyhow::anyhow!("Missing symbol _earlyboot_memset"))?
    .address();
  let entry_hook_addr = symbol_map.get(&mod_data.config.entry_point_symbol)
    .ok_or_else(|| anyhow::anyhow!("Missing symbol {}", mod_data.config.entry_point_symbol))?
    .address();

  let mut output_bytes = dol_bytes.to_vec();

  for segment in mod_file.segments() {
    // find the sections that are part of this segment
    let name = segment.name();
    let segment_range = segment.address()..(segment.address() + segment.size());
    info!("Segment: {:?}", name);
    for section in mod_file.sections() {
      if segment_range.contains(&section.address()) {
        let section_name = section.name().unwrap_or("<unnamed>");
        info!("  - {:} @ 0x{:08X} - 0x{:08X} ({:} bytes)",
                      section_name,
                      section.address(),
                      section.address() + section.size(),
                      section.size());
      }
    }
    let data = segment.data()?;
    info!("Data size: {} bytes", data.len());
    if data.is_empty() {
      info!("Skipping empty segment");
      continue;
    }

    let segment_output_offset = output_bytes.len();
    output_bytes.extend_from_slice(&data);
    info!("Wrote segment data at output offset 0x{:08X}", segment_output_offset);

    // find a target section in the .dol with an offset of 0
    let mut found = false;
    for dol_segment in dol_header.text.iter_mut().chain(dol_header.data.iter_mut()) {
      if dol_segment.offset != 0 {
        continue;
      }
      found = true;
      dol_segment.offset = segment_output_offset as u32;
      dol_segment.loading = segment.address() as u32;
      dol_segment.size = segment.size() as u32;
      info!("Patching DOL segment offset 0x{:08X} loading 0x{:08X} size 0x{:08X} end 0x{:08X}",
            dol_segment.offset,
            dol_segment.loading,
            dol_segment.size,
            dol_segment.loading + dol_segment.size);
      break;
    }
    if !found {
      return Err(anyhow::anyhow!("No available DOL segment found for mod segment"));
    }
  }

  info!("Updating DOL header");
  {
    let mut cursor = io::Cursor::new(&mut output_bytes[..]);
    dol_header.write_to_stream(&mut cursor)?;
  }

  let arenahi_upper = ((link_end >> 16) & 0xFFFF) as u32;
  let areenalo_lower = (link_end & 0xFFFF) as u32;
  patch_dol_addr_32(&dol_header, &mut output_bytes, patch_arena_lo_1 as u32, |x| {
    (x & 0xFFFF_0000) | arenahi_upper
  })?;
  patch_dol_addr_32(&dol_header, &mut output_bytes, patch_arena_lo_1 as u32 + 4, |x| {
    (x & 0xFFFF_0000) | areenalo_lower
  })?;
  patch_dol_addr_32(&dol_header, &mut output_bytes, patch_arena_lo_2 as u32, |x| {
    (x & 0xFFFF_0000) | arenahi_upper
  })?;
  patch_dol_addr_32(&dol_header, &mut output_bytes, patch_arena_lo_2 as u32 + 4, |x| {
    (x & 0xFFFF_0000) | areenalo_lower
  })?;
  patch_dol_addr_32(&dol_header, &mut output_bytes, entry_hook_addr as u32, |_| {
    build_b_rel24(entry_hook_addr as u32, entry_addr as u32)
  })?;
  patch_dol_addr_32(&dol_header, &mut output_bytes, patch_earlyboot_memset as u32, |_| {
    build_b_rel24(patch_earlyboot_memset as u32, patch_earlyboot_memset_addr as u32)
  })?;


  Ok(output_bytes)
}

fn patch_dol_addr_32<F>(
  dol_header: &DolHeader,
  dol_bytes: &mut Vec<u8>,
  addr: u32,
  function: F,
) -> Result<()>
where
  F: Fn(u32) -> u32,
{
  let mut found_count = 0;
  for dol_segment in dol_header.text.iter().chain(dol_header.data.iter()) {
    if dol_segment.loading <= addr && addr < (dol_segment.loading + dol_segment.size) {
      found_count += 1;
      let seg_offset = addr - dol_segment.loading;
      let offset = dol_segment.offset + seg_offset;
      let current = {
        let mut cursor = io::Cursor::new(&dol_bytes[..]);
        cursor.set_position(offset as u64);
        cursor.read_u32()?
      };
      let new = function(current);
      info!("Patching 0x{:08X} (0x{:08X}) from 0x{:08X} -> 0x{:08X}",
            offset,
            addr,
            current,
            new);
      {
        let mut cursor = io::Cursor::new(&mut dol_bytes[..]);
        cursor.set_position(offset as u64);
        cursor.write_u32(new)?;
      }
      // this may happen in multiple segments
    }
  }
  if found_count > 0 {
    Ok(())
  } else {
    Err(anyhow::anyhow!("Address 0x{:08X} not found in DOL segments", addr))
  }
}

fn build_b_rel24(addr: u32, target: u32) -> u32 {
  let rel = (target.wrapping_sub(addr)) & 0xFFFF_FFFC;
  let op = 0x4800_0000;
  op | rel
}