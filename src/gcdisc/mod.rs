use crate::binser::binstream::{BinStreamRead, BinStreamReadable, BinStreamWritable, BinStreamWrite};
use std::io::Write;

mod fst;
mod gc_disc_header;

pub use fst::*;
pub use gc_disc_header::*;