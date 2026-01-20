use std::io::{Read, Seek, SeekFrom};

pub mod binstream;

pub struct SubstreamReader<R: Read + Seek> {
  inner: R,
  start: u64,
  length: u64,
}

impl<R: Read + Seek> SubstreamReader<R> {
  pub fn new(mut inner: R, start: u64, length: u64) -> std::io::Result<Self> {
    inner.seek(SeekFrom::Start(start))?;
    Ok(SubstreamReader {
      inner,
      start,
      length,
    })
  }
  
  pub fn new_from_current(mut inner: R, length: u64) -> std::io::Result<Self> {
    let start = inner.seek(SeekFrom::Current(0))?;
    Ok(SubstreamReader {
      inner,
      start,
      length,
    })
  }
}

impl<R: Read + Seek> Read for SubstreamReader<R> {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    let current_pos = self.inner.seek(SeekFrom::Current(0))?;
    let bytes_left = self.length.saturating_sub(current_pos - self.start);
    if bytes_left == 0 {
      return Ok(0);
    }
    let to_read = std::cmp::min(buf.len() as u64, bytes_left) as usize;
    self.inner.read(&mut buf[..to_read])
  }
}

impl<R: Read + Seek> Seek for SubstreamReader<R> {
  fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
    let new_pos = match pos {
      SeekFrom::Start(offset) => {
        if offset > self.length {
          return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Seek out of bounds"));
        }
        self.inner.seek(SeekFrom::Start(self.start + offset))?
      }
      SeekFrom::End(offset) => {
        let end_pos = self.start + self.length;
        let target_pos = if offset >= 0 {
          end_pos.checked_add(offset as u64)
        } else {
          end_pos.checked_sub((-offset) as u64)
        };
        match target_pos {
          Some(pos) if pos >= self.start && pos <= end_pos => self.inner.seek(SeekFrom::Start(pos))?,
          _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Seek out of bounds")),
        }
      }
      SeekFrom::Current(offset) => {
        let current_pos = self.inner.seek(SeekFrom::Current(0))?;
        let target_pos = if offset >= 0 {
          current_pos.checked_add(offset as u64)
        } else {
          current_pos.checked_sub((-offset) as u64)
        };
        match target_pos {
          Some(pos) if pos >= self.start && pos <= self.start + self.length => self.inner.seek(SeekFrom::Start(pos))?,
          _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Seek out of bounds")),
        }
      }
    };
    Ok(new_pos - self.start)
  }
}