use crate::binser::binstream::{BinStreamRead, BinStreamReadable, BinStreamWritable, BinStreamWrite};

#[derive(Clone, Debug)]
pub struct GCDiscHeader {
  pub code: u32,
  pub maker_code: u16,
  pub disk_id: u8,
  pub version: u8,
  pub audio_streaming: u8,
  pub streaming_buffer_size: u8,
  pub unused_1: Vec<u8>,
  pub magic_word: u32,
  pub game_name: Vec<u8>,
  pub debug_monitor: u32,
  pub debug_monitor_load: u32,
  pub unused_2: Vec<u8>,
  pub dol_offset: u32,
  pub fst_offset: u32,
  pub fst_size: u32,
  pub fst_max_size: u32,
  pub user_pos: u32,
  pub user_len: u32,
  pub unused_3: u32,
  pub unused_4: u32,
}

impl GCDiscHeader {
  pub fn name_string(&self) -> String {
    // format: <code decoded as ascii><maker_code decoded as ascii>: <game_name decoded as ascii, trimmed>
    let code_str = String::from_utf8_lossy(&self.code.to_be_bytes()).to_string();
    let maker_code_str = String::from_utf8_lossy(&self.maker_code.to_be_bytes()).to_string();
    let game_name_str = String::from_utf8_lossy(&self.game_name)
      .trim_end_matches(char::from(0))
      .to_string();
    format!("{}{}: {}", code_str, maker_code_str, game_name_str)
  }
}

impl BinStreamReadable for GCDiscHeader {
  fn read_from_stream<T: BinStreamRead>(stream: &mut T) -> crate::binser::binstream::Result<Self> {
    let code = stream.read_u32()?;
    let maker_code = stream.read_u16()?;
    let disk_id = stream.read_u8()?;
    let version = stream.read_u8()?;
    let audio_streaming = stream.read_u8()?;
    let streaming_buffer_size = stream.read_u8()?;
    let unused_1 = stream.read_bytes(0x12)?;
    let magic_word = stream.read_u32()?;
    let game_name = stream.read_bytes(0x3E0)?;
    let debug_monitor = stream.read_u32()?;
    let debug_monitor_load = stream.read_u32()?;
    let unused_2 = stream.read_bytes(0x18)?;
    let dol_offset = stream.read_u32()?;
    let fst_offset = stream.read_u32()?;
    let fst_size = stream.read_u32()?;
    let fst_max_size = stream.read_u32()?;
    let user_pos = stream.read_u32()?;
    let user_len = stream.read_u32()?;
    let unused_3 = stream.read_u32()?;
    let unused_4 = stream.read_u32()?;

    Ok(GCDiscHeader {
      code,
      maker_code,
      disk_id,
      version,
      audio_streaming,
      streaming_buffer_size,
      unused_1,
      magic_word,
      game_name,
      debug_monitor,
      debug_monitor_load,
      unused_2,
      dol_offset,
      fst_offset,
      fst_size,
      fst_max_size,
      user_pos,
      user_len,
      unused_3,
      unused_4,
    })
  }
}

impl BinStreamWritable for GCDiscHeader {
  fn write_to_stream<T: BinStreamWrite>(&self, stream: &mut T) -> crate::binser::binstream::Result<()> {
    stream.write_u32(self.code)?;
    stream.write_u16(self.maker_code)?;
    stream.write_u8(self.disk_id)?;
    stream.write_u8(self.version)?;
    stream.write_u8(self.audio_streaming)?;
    stream.write_u8(self.streaming_buffer_size)?;
    stream.write(&self.unused_1)?;
    stream.write_u32(self.magic_word)?;
    stream.write(&self.game_name)?;
    stream.write_u32(self.debug_monitor)?;
    stream.write_u32(self.debug_monitor_load)?;
    stream.write(&self.unused_2)?;
    stream.write_u32(self.dol_offset)?;
    stream.write_u32(self.fst_offset)?;
    stream.write_u32(self.fst_size)?;
    stream.write_u32(self.fst_max_size)?;
    stream.write_u32(self.user_pos)?;
    stream.write_u32(self.user_len)?;
    stream.write_u32(self.unused_3)?;
    stream.write_u32(self.unused_4)?;

    Ok(())
  }
}