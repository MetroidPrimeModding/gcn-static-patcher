#![allow(dead_code)]

use std::io;
use std::io::{Read, Seek, Write};

pub type Result<T> = io::Result<T>;

pub trait BinStreamRead: Read + Seek + Sized {
  fn read_bytes(&mut self, size: u32) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; size as usize];
    self.read(&mut buf)?;
    Ok(buf)
  }

  fn read_u8(&mut self) -> Result<u8> {
    let mut buf = [0u8; 1];
    self.read_exact(&mut buf)?;
    Ok(buf[0])
  }

  fn read_u16(&mut self) -> Result<u16> {
    let mut buf = [0u8; 2];
    self.read_exact(&mut buf)?;
    Ok(u16::from_be_bytes(buf))
  }

  fn read_u32(&mut self) -> Result<u32> {
    let mut buf = [0u8; 4];
    self.read_exact(&mut buf)?;
    Ok(u32::from_be_bytes(buf))
  }

  fn read_u64(&mut self) -> Result<u64> {
    let mut buf = [0u8; 8];
    self.read_exact(&mut buf)?;
    Ok(u64::from_be_bytes(buf))
  }

  fn read_f32(&mut self) -> Result<f32> {
    let mut buf = [0u8; 4];
    self.read_exact(&mut buf)?;
    Ok(f32::from_be_bytes(buf))
  }

  fn read_f64(&mut self) -> Result<f64> {
    let mut buf = [0u8; 8];
    self.read_exact(&mut buf)?;
    Ok(f64::from_be_bytes(buf))
  }

  fn read_string(&mut self, size: usize) -> Result<String> {
    if size > u32::MAX as usize {
      return Err(io::Error::other("String size too large"));
    }
    let buf = self.read_bytes(size as u32)?;
    String::from_utf8(buf)
      .map_err(|e| io::Error::other(e.to_string()))
  }

  fn read_string_sized(&mut self) -> Result<String> {
    let size = self.read_u32()?;
    self.read_string(size as usize)
  }

  fn read_object<T: BinStreamReadable>(&mut self) -> Result<T> {
    T::read_from_stream(self)
  }
}

pub trait BinStreamWrite: Write + Seek + Sized {
  fn write_u8(&mut self, value: u8) -> Result<usize> {
    let buf = [value];
    self.write(&buf)
  }

  fn write_u16(&mut self, value: u16) -> Result<usize> {
    let buf = value.to_be_bytes();
    self.write(&buf)
  }

  fn write_u32(&mut self, value: u32) -> Result<usize> {
    let buf = value.to_be_bytes();
    self.write(&buf)
  }

  fn write_u64(&mut self, value: u64) -> Result<usize> {
    let buf = value.to_be_bytes();
    self.write(&buf)
  }

  fn write_f32(&mut self, value: f32) -> Result<usize> {
    let buf = value.to_be_bytes();
    self.write(&buf)
  }

  fn write_f64(&mut self, value: f64) -> Result<usize> {
    let buf = value.to_be_bytes();
    self.write(&buf)
  }

  fn write_string(&mut self, value: &str) -> Result<usize> {
    self.write(value.as_bytes())
  }

  fn write_string_sized(&mut self, value: &str) -> Result<usize> {
    let size = value.len() as u32;
    if size > u32::MAX {
      return Err(io::Error::other("String size too large"));
    }
    self.write_u32(size)?;
    self.write(value.as_bytes())
  }

  fn write_object<T: BinStreamWritable>(&mut self, obj: T) -> Result<()> {
    obj.write_to_stream(self)
  }
}

impl<T> BinStreamRead for T
where
  T: Read + Seek,
{
}

impl<T> BinStreamWrite for T
where
  T: Write + Seek,
{
}

pub trait BinStreamReadable: Sized {
  fn read_from_stream<T: BinStreamRead>(stream: &mut T) -> Result<Self>;
}

pub trait BinStreamWritable: Sized {
  fn write_to_stream<T: BinStreamWrite>(&self, stream: &mut T) -> Result<()>;
}
