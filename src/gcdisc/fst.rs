use std::fmt::{Debug, Formatter};
use std::io::SeekFrom;
use crate::binser::binstream::{BinStreamRead, BinStreamReadable, BinStreamWritable, BinStreamWrite};

#[derive(Clone)]
pub enum FSTEntry {
  Directory {
    name: String,
    children: Option<Vec<FSTEntry>>,
  },
  File {
    name: String,
    offset: Option<u32>,
    length: Option<u32>,
  }
}

#[derive(Clone)]
pub struct FST {
  pub root: FSTEntry,
}

#[derive(Clone, Debug)]
pub struct FSTEntryData {
  pub directory: bool,
  pub filename: u32,
  pub offset: u32,
  pub length: u32,
}

impl Debug for FST {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    self.root.print(&vec![], f)
  }
}

impl FSTEntry {
  fn print(&self, parents: &Vec<&str>, f: &mut Formatter<'_>) -> std::fmt::Result {
    let mut path_parts = parents.clone();
    match self {
      FSTEntry::Directory { name, children } => {
        path_parts.push(name);
        writeln!(f, "{}/", path_parts.join("/"))?;
        if let Some(children) = children {
          for child in children {
            child.print(&path_parts, f)?;
          }
        }
        Ok(())
      }
      FSTEntry::File { name, offset, length } => {
        path_parts.push(name);
        writeln!(
          f,
          "{} {:08X} {:?}",
          path_parts.join("/"),
          offset.unwrap_or(0),
          length
        )
      }
    }
  }

  pub fn get_ranges(&self) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    match self {
      FSTEntry::Directory { children, .. } => {
        if let Some(children) = children {
          for child in children {
            ranges.extend(child.get_ranges());
          }
        }
      }
      FSTEntry::File { offset, length, .. } => {
        if let (Some(off), Some(len)) = (offset, length) {
          ranges.push((*off, *off + *len));
        }
      }
    }
    ranges.sort_by_key(|a| a.0);
    ranges
  }

  pub fn find(&self, path: &[&str]) -> Option<&FSTEntry> {
    if path.is_empty() {
      return None;
    }
    let head = path[0];
    let tail = &path[1..];
    match self {
      FSTEntry::Directory { name, children } => {
        if head != name {
          return None;
        }
        if let Some(children) = children {
          for child in children {
            if let Some(found) = child.find(tail) {
              return Some(found);
            }
          }
        }
        None
      }
      FSTEntry::File { name, .. } => {
        if head == name && tail.is_empty() {
          Some(self)
        } else {
          None
        }
      }
    }
  }

  pub fn count(&self) -> u32 {
    match self {
      FSTEntry::Directory { children, .. } => {
        let mut count = 1; // count the directory itself
        if let Some(children) = children {
          for child in children {
            count += child.count();
          }
        }
        count
      }
      FSTEntry::File { .. } => 1,
    }
  }
}

impl BinStreamReadable for FST {
  fn read_from_stream<T: BinStreamRead>(stream: &mut T) -> crate::binser::binstream::Result<Self> {
    fn read_entry_data<T: BinStreamRead>(stream: &mut T) -> crate::binser::binstream::Result<FSTEntryData> {
      let name_and_type = stream.read_u32()?;
      let directory = (name_and_type & 0xFF00_0000) != 0;
      let filename = name_and_type & 0x00FF_FFFF;
      let offset = stream.read_u32()?;
      let length = stream.read_u32()?;

      Ok(FSTEntryData {
        directory,
        filename,
        offset,
        length,
      })
    }

    fn read_cstring<T: BinStreamRead>(
      stream: &mut T,
      base: u64,
      offset: u32,
      max_len: usize,
    ) -> crate::binser::binstream::Result<String> {
      let current_pos = stream.seek(SeekFrom::Current(0))?;
      stream.seek(SeekFrom::Start(base + offset as u64))?;
      let mut buf = Vec::new();
      for _ in 0..max_len {
        let byte = stream.read_u8()?;
        if byte == 0 {
          break;
        }
        buf.push(byte);
      }
      stream.seek(SeekFrom::Start(current_pos))?;
      String::from_utf8(buf)
        .map_err(|e| std::io::Error::other(e.to_string()))
    }

    enum TempNode {
      Directory {
        name: String,
        children: Vec<usize>,
        offset_of_next_node: Option<u32>,
      },
      File {
        name: String,
        offset: u32,
        length: u32,
      },
    }

    fn build_entry(nodes: &[TempNode], index: usize) -> FSTEntry {
      match &nodes[index] {
        TempNode::Directory { name, children, .. } => {
          let mut built_children = Vec::with_capacity(children.len());
          for &child_index in children {
            built_children.push(build_entry(nodes, child_index));
          }
          FSTEntry::Directory {
            name: name.clone(),
            children: Some(built_children),
          }
        }
        TempNode::File { name, offset, length } => FSTEntry::File {
          name: name.clone(),
          offset: Some(*offset),
          length: Some(*length),
        },
      }
    }

    let start = stream.seek(SeekFrom::Current(0))?;
    let root_data = read_entry_data(stream)?;
    let count = root_data.length;
    let mut entry_datas = Vec::with_capacity(count.saturating_sub(1) as usize);
    for _ in 1..count {
      entry_datas.push(read_entry_data(stream)?);
    }

    let string_table_start = start + (count as u64) * 0xC;
    let root_name = read_cstring(stream, string_table_start, root_data.filename, 256)?;
    let mut max_string_end = root_data.filename as u64 + root_name.as_bytes().len() as u64 + 1;

    let mut entries = Vec::with_capacity(count as usize);
    entries.push(TempNode::Directory {
      name: root_name,
      children: Vec::new(),
      offset_of_next_node: None,
    });

    let mut directory_stack = vec![0usize];
    let mut next_dir_offset: Option<u32> = None;

    for entry_data in entry_datas {
      let offset = entries.len() as u32;
      if Some(offset) == next_dir_offset {
        directory_stack.pop();
        next_dir_offset = directory_stack
          .last()
          .and_then(|&index| match &entries[index] {
            TempNode::Directory { offset_of_next_node, .. } => *offset_of_next_node,
            TempNode::File { .. } => None,
          });
      }

      let name = read_cstring(stream, string_table_start, entry_data.filename, 256)?;
      let name_end = entry_data.filename as u64 + name.as_bytes().len() as u64 + 1;
      if name_end > max_string_end {
        max_string_end = name_end;
      }

      if entry_data.directory {
        let entry_index = entries.len();
        entries.push(TempNode::Directory {
          name,
          children: Vec::new(),
          offset_of_next_node: Some(entry_data.length),
        });
        if let Some(&parent_index) = directory_stack.last() {
          if let TempNode::Directory { children, .. } = &mut entries[parent_index] {
            children.push(entry_index);
          }
        }
        directory_stack.push(entry_index);
        next_dir_offset = Some(entry_data.length);
      } else {
        let entry_index = entries.len();
        entries.push(TempNode::File {
          name,
          offset: entry_data.offset,
          length: entry_data.length,
        });
        if let Some(&parent_index) = directory_stack.last() {
          if let TempNode::Directory { children, .. } = &mut entries[parent_index] {
            children.push(entry_index);
          }
        }
      }
    }

    let root = build_entry(&entries, 0);
    let total_len = (count as u64) * 0xC + max_string_end;
    stream.seek(SeekFrom::Start(start + total_len))?;

    Ok(FST { root })
  }
}

impl BinStreamWritable for FST {
  fn write_to_stream<T: BinStreamWrite>(&self, stream: &mut T) -> crate::binser::binstream::Result<()> {
    fn write_u32_at<T: BinStreamWrite>(
      stream: &mut T,
      base: u64,
      offset: u64,
      value: u32,
    ) -> crate::binser::binstream::Result<()> {
      stream.seek(SeekFrom::Start(base + offset))?;
      stream.write_u32(value)?;
      Ok(())
    }

    fn write_entry<T: BinStreamWrite>(
      entry: &FSTEntry,
      stream: &mut T,
      base: u64,
      string_table_start: u64,
      file_offset: &mut u32,
      string_offset: &mut u32,
      parent_index: Option<u32>,
      total_count: u32,
    ) -> crate::binser::binstream::Result<()> {
      let name = match entry {
        FSTEntry::Directory { name, .. } => name,
        FSTEntry::File { name, .. } => name,
      };

      let name_offset = *string_offset;
      stream.seek(SeekFrom::Start(string_table_start + name_offset as u64))?;
      stream.write_string(name)?;
      stream.write_u8(0)?;
      *string_offset += name.as_bytes().len() as u32 + 1;

      let my_offset = *file_offset;
      *file_offset += 1;
      let my_byte_offset = (my_offset as u64) * 0xC;

      match entry {
        FSTEntry::Directory { children, .. } => {
          let dir_header = (0x01u32 << 24) | name_offset;
          write_u32_at(stream, base, my_byte_offset + 0x0, dir_header)?;
          if let Some(parent) = parent_index {
            write_u32_at(stream, base, my_byte_offset + 0x4, parent)?;
            let child_count = children.as_ref().map_or(0u32, |c| c.len() as u32);
            write_u32_at(stream, base, my_byte_offset + 0x8, my_offset + child_count + 1)?;
          } else {
            write_u32_at(stream, base, my_byte_offset + 0x4, 0)?;
            write_u32_at(stream, base, my_byte_offset + 0x8, total_count)?;
          }

          if let Some(children) = children {
            for child in children.iter().filter(|c| matches!(c, FSTEntry::File { .. })) {
              write_entry(
                child,
                stream,
                base,
                string_table_start,
                file_offset,
                string_offset,
                Some(my_offset),
                total_count,
              )?;
            }
            for child in children.iter().filter(|c| matches!(c, FSTEntry::Directory { .. })) {
              write_entry(
                child,
                stream,
                base,
                string_table_start,
                file_offset,
                string_offset,
                Some(my_offset),
                total_count,
              )?;
            }
          }
        }
        FSTEntry::File { offset, length, .. } => {
          write_u32_at(stream, base, my_byte_offset + 0x0, name_offset)?;
          write_u32_at(stream, base, my_byte_offset + 0x4, offset.unwrap_or(0))?;
          write_u32_at(stream, base, my_byte_offset + 0x8, length.unwrap_or(0))?;
        }
      }

      Ok(())
    }

    let base = stream.seek(SeekFrom::Current(0))?;
    let total_count = self.root.count();
    let string_table_start = base + (total_count as u64) * 0xC;
    let mut file_offset = 0u32;
    let mut string_offset = 0u32;

    write_entry(
      &self.root,
      stream,
      base,
      string_table_start,
      &mut file_offset,
      &mut string_offset,
      None,
      total_count,
    )?;

    let total_len = (total_count as u64) * 0xC + string_offset as u64;
    stream.seek(SeekFrom::Start(base + total_len))?;
    Ok(())
  }
}

impl BinStreamReadable for FSTEntryData {
  fn read_from_stream<T: BinStreamRead>(stream: &mut T) -> crate::binser::binstream::Result<Self> {
    let directory = stream.read_u8()? != 0;
    let filename = stream.read_u32()? & 0x00FF_FFFF;
    let offset = stream.read_u32()?;
    let length = stream.read_u32()?;

    Ok(FSTEntryData {
      directory,
      filename,
      offset,
      length,
    })
  }
}

impl BinStreamWritable for FSTEntryData {
  fn write_to_stream<T: BinStreamWrite>(&self, stream: &mut T) -> crate::binser::binstream::Result<()> {
    let dir_byte = if self.directory { 1u8 } else { 0u8 };
    stream.write_u8(dir_byte)?;
    stream.write_u32(self.filename)?;
    stream.write_u32(self.offset)?;
    stream.write_u32(self.length)?;
    Ok(())
  }
}
