use crate::binstream::{BinStreamRead, BinStreamReadable, BinStreamWritable, BinStreamWrite};
use std::fmt;
use std::fmt::Debug;

#[derive(Clone)]
pub struct DolHeader {
  pub text: Vec<SectionInfo>,
  pub data: Vec<SectionInfo>,
  pub bss_addr: u32,
  pub bss_size: u32,
  pub entry_point: u32,
}

#[derive(Clone)]
pub struct SectionInfo {
  pub offset: u32,
  pub loading: u32,
  pub size: u32,
}

impl DolHeader {
  pub fn total_length(&self) -> u32 {
    let mut max_end = 0;
    for seg in self.text.iter().chain(self.data.iter()) {
      let end = seg.offset + seg.size;
      if end > max_end {
        max_end = end;
      }
    }
    max_end
  }
}

impl Debug for DolHeader {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "DOL Header:")?;
    for (i, seg) in self.text.iter().enumerate() {
      if seg.size == 0 {
        continue;
      }
      writeln!(
        f,
        " Text Segment {}: Offset: 0x{:08X}, Loading: 0x{:08X}, Size: 0x{:08X}, End: 0x{:08X}",
        i,
        seg.offset,
        seg.loading,
        seg.size,
        seg.loading + seg.size
      )?;
    }
    for (i, seg) in self.data.iter().enumerate() {
      if seg.size == 0 {
        continue;
      }
      writeln!(
        f,
        " Data Segment {}: Offset: 0x{:08X}, Loading: 0x{:08X}, Size: 0x{:08X}, End: 0x{:08X}",
        i,
        seg.offset,
        seg.loading,
        seg.size,
        seg.loading + seg.size
      )?;
    }
    writeln!(
      f,
      " BSS Address: 0x{:08X}, BSS Size: 0x{:08X}, end: 0x{:08X}",
      self.bss_addr,
      self.bss_size,
      self.bss_addr + self.bss_size
    )?;
    writeln!(f, " Entry Point: 0x{:08X}", self.entry_point)
  }
}

impl BinStreamReadable for DolHeader {
  fn read_from_stream<T: BinStreamRead>(
    stream: &mut T,
  ) -> std::io::Result<Self> {
    let mut text_offsets = Vec::with_capacity(7);
    for _ in 0..7 { text_offsets.push(stream.read_u32()?); }
    let mut data_offsets = Vec::with_capacity(11);
    for _ in 0..11 { data_offsets.push(stream.read_u32()?); }
    let mut text_addrs = Vec::with_capacity(7);
    for _ in 0..7 { text_addrs.push(stream.read_u32()?); }
    let mut data_addrs = Vec::with_capacity(11);
    for _ in 0..11 { data_addrs.push(stream.read_u32()?); }
    let mut text_sizes = Vec::with_capacity(7);
    for _ in 0..7 { text_sizes.push(stream.read_u32()?); }
    let mut data_sizes = Vec::with_capacity(11);
    for _ in 0..11 { data_sizes.push(stream.read_u32()?); }

    let bss_addr = stream.read_u32()?;
    let bss_size = stream.read_u32()?;
    let entry_point = stream.read_u32()?;

    let text: Vec<_> = (0..7)
      .map(|i| SectionInfo {
        offset: text_offsets[i],
        loading: text_addrs[i],
        size: text_sizes[i],
      })
      .collect();
    let data: Vec<_> = (0..11)
      .map(|i| SectionInfo {
        offset: data_offsets[i],
        loading: data_addrs[i],
        size: data_sizes[i],
      })
      .collect();

    Ok(DolHeader {
      text,
      data,
      bss_addr,
      bss_size,
      entry_point,
    })
  }
}

impl BinStreamWritable for DolHeader {
  fn write_to_stream<T: BinStreamWrite>(
    &self,
    stream: &mut T,
  ) -> std::io::Result<()> {
    for seg in &self.text {
      stream.write_u32(seg.offset)?;
    }
    for seg in &self.data {
      stream.write_u32(seg.offset)?;
    }
    for seg in &self.text {
      stream.write_u32(seg.loading)?;
    }
    for seg in &self.data {
      stream.write_u32(seg.loading)?;
    }
    for seg in &self.text {
      stream.write_u32(seg.size)?;
    }
    for seg in &self.data {
      stream.write_u32(seg.size)?;
    }
    stream.write_u32(self.bss_addr)?;
    stream.write_u32(self.bss_size)?;
    stream.write_u32(self.entry_point)?;
    Ok(())
  }
}