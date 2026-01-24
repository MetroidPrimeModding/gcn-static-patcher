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

  fn write_string(&mut self, value: &str) -> Result<usize> {
    self.write(value.as_bytes())
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
