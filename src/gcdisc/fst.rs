/*

@dataclass
class FSTEntry:
    directory: bool
    name: str
    children: list = None
    offset: int = None
    length: int = None

    def print(self, parents=[]):
        # print("\t" * depth + f"{self.name} {(self.offset or 0):08x} {self.length}")
        print(f"{'/'.join(parents + [self.name])} {(self.offset or 0):08X} {self.length}")
        if self.children:
            for child in self.children:
                child.print(parents + [self.name])

    def get_ranges(self):
        ranges = []
        if self.directory:
            for child in self.children:
                ranges += child.get_ranges()
        else:
            ranges.append((self.offset, self.offset + self.length))
        ranges.sort(key=lambda a: a[0])
        return ranges

    def find(self, path: [str]):
        head = path[0]
        tail = path[1:]
        if head != self.name:
            return None
        if self.directory:
            for child in self.children:
                found = child.find(tail)
                if found:
                    return found
        else:
            return self

    def find_offset(self, offset: int):
        if self.directory:
            print("Searching in: " + self.name)
            for child in self.children:
                found = child.find_offset(offset)
                if found:
                    return found
            return None
        else:
            print(f"Checking file: {self.name} @ {self.offset:08X}-{(self.offset + self.length):08X} for offset {offset:08X}")
            if self.offset == offset:
                return self
            else:
                return None

    def count(self):
        if self.directory:
            count = 1
            for child in self.children:
                count += child.count()
            return count
        else:
            return 1


class FSTRecursiveWriter:
    def __init__(self, fst):
        self.fst = fst
        self.file_offset = 0
        self.string_offset = 0
        self.str_table: DataWriter

    def write(self, dest: DataWriter):
        self.count = self.fst.count()
        self.str_table = dest.with_offset(self.count * 0xC)
        self.write_recursively(self.fst.root, dest, -1)
        self.len = self.count * 0xC + self.string_offset

    def write_recursively(self, entry: FSTEntry, dest: DataWriter, parent_index: int):
        # Write name
        name_offset = self.string_offset
        self.str_table.write_string(name_offset, entry.name)
        self.string_offset += len(entry.name) + 1

        my_offset = self.file_offset
        self.file_offset += 1

        my_byte_offset = my_offset * 0xC

        if entry.directory:
            if parent_index < 0:
                # root
                dest.write_u32(my_byte_offset + 0x0, (0x01 << 24) | name_offset)
                dest.write_u32(my_byte_offset + 0x4, 0)
                dest.write_u32(my_byte_offset + 0x8, self.count)
            else:
                dest.write_u32(my_byte_offset + 0x0, (0x01 << 24) | name_offset)
                dest.write_u32(my_byte_offset + 0x4, parent_index)
                dest.write_u32(my_byte_offset + 0x8, my_offset + len(entry.children) + 1)
            for child in [x for x in entry.children if not x.directory]:
                self.write_recursively(child, dest, my_offset)
            for child in [x for x in entry.children if x.directory]:
                self.write_recursively(child, dest, my_offset)
        else:
            dest.write_u32(my_byte_offset + 0x0, name_offset)
            dest.write_u32(my_byte_offset + 0x4, entry.offset)
            dest.write_u32(my_byte_offset + 0x8, entry.length)


@dataclass
class FST:
    root: FSTEntry

    @staticmethod
    def parse(src: DataReader):
        root_data = FSTEntryData.parse(src)
        entry_datas = []
        for i in range(1, root_data.length):
            entry_datas.append(FSTEntryData.parse(src.with_offset(i * 0xC)))
        string_table = src.with_offset(root_data.length * 0xC)

        root_entry = FSTEntry(
            directory=True,
            name=string_table.read_string(root_data.filename),
            children=[]
        )
        entries = [root_entry]
        directory_stack = [root_entry]
        next_dir_offset = None
        for entry_data in entry_datas:
            offset = len(entries)
            if offset == next_dir_offset:
                directory_stack.pop()
                next_dir_offset = directory_stack[-1].length

            if entry_data.directory:
                entry = FSTEntry(
                    directory=True,
                    name=string_table.read_string(entry_data.filename),
                    children=[],
                    length=entry_data.length
                )
                entries.append(entry)
                parent = directory_stack[-1]
                directory_stack.append(entry)
                next_dir_offset = entry_data.length
                parent.children.append(entry)
            else:
                entry = FSTEntry(
                    directory=False,
                    name=string_table.read_string(entry_data.filename),
                    offset=entry_data.offset,
                    length=entry_data.length
                )
                entries.append(entry)
                parent = directory_stack[-1]
                parent.children.append(entry)
        return FST(root=root_entry)

    def write(self, dest: DataWriter):
        writer = FSTRecursiveWriter(self)
        writer.write(dest)
        return writer.len

    def print(self):
        self.root.print()

    def get_ranges(self):
        return self.root.get_ranges()

    def find(self, path: [str]):
        return self.root.find([self.root.name] + path)

    def find_offset(self, offset: int):
        return self.root.find_offset(offset)

    def count(self):
        return self.root.count()


@dataclass
class FSTEntryData:
    directory: int
    filename: int
    offset: int
    length: int

    @staticmethod
    def parse(src: DataReader):
        return FSTEntryData(
            directory=src.read_u8(0x0) != 0,
            filename=src.read_u32(0x0) & 0x00FF_FFFF,
            offset=src.read_u32(0x4),
            length=src.read_u32(0x8),
        )
*/
use std::fmt::{Debug, Formatter};
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
    todo!("Implement FST reading from stream")
  }
}

impl BinStreamWritable for FST {
  fn write_to_stream<T: BinStreamWrite>(&self, stream: &mut T) -> crate::binser::binstream::Result<()> {
    todo!("Implement FST writing to stream")
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