use binrw::io::BufReader;
use binrw::BinRead;
use std::fs::File;
use std::io::{Cursor, Read};
use std::sync::atomic::AtomicBool;

#[derive(BinRead, Debug)]
#[br(big, import{mag: Vec<u8>})]
pub struct PerfFileSection {
    #[br(is_little = (mag == b"PERFILE2"))]
    pub offset: u64,
    #[br(is_little = (mag == b"PERFILE2"))]
    pub size: u64,
}

/// Header of perf file. Contains information on data read later on.
/// DO NOT USE MULTILINE DOCCOMMENT SYNTAX (breaks binrw macro lol)
#[derive(BinRead, Debug)]
#[br(big)]
pub struct PerfFileHeader {
    #[br(count = 8)]
    pub mag: Vec<u8>,
    /// Size of this header.
    #[br(is_little = (mag == *b"PERFILE2"))]
    pub size: u64,
    /// Size of one attribute section, if it does not match, the entries may need to be swapped.
    /// We assume that it matches.
    #[br(is_little = (mag == *b"PERFILE2"))]
    pub attr_size: u64,
    // List of perf file attr entries
    #[br(count = attr_size, args {inner: PerfFileSectionBinReadArgs{mag:(mag.clone())}})]
    pub attrs: Vec<PerfFileSection>,
    /// "See Section 3.2" aka -- sheer pain ahead.
    #[br(is_little = (mag == *b"PERFILE2"), args {mag:mag.clone()})]
    pub data: PerfFileSection,
    /// List of perf trace event type entries
    #[br(args {mag:mag.clone()})]
    pub event_types: PerfFileSection,
    #[br(is_little = (mag == *b"PERFILE2"))]
    pub flags: u64,
    #[br(is_little = (mag == *b"PERFILE2"), count = 3)]
    pub misc: Vec<u64>,
}

// if ever read this, deal with endianness then.
#[derive(BinRead, Debug)]
#[br(big)]
pub struct PerfHeaderString {
    pub len: u32,
    #[br(count = len)]
    pub string: Vec<u8>,
}

// // if ever read this, deal with endianness then.
// #[derive(BinRead, Debug)]
// #[br(big)]
// pub struct PerfHeaderStringList {
//     pub nr: u32,
//     #[br(count = nr)]
//     pub string: Vec<String>,
// }

fn main() -> Result<(), anyhow::Error> {
    let path = "/Users/patsomaru/Documents/GitHub/sto/tests/perf.data";
    let file = File::open(path).expect("Cannot read file.");
    let mut buf = BufReader::new(file);
    let read = PerfFileHeader::read(&mut buf);
    // let mut buf = BufReader::new(file);
    // use nom::number::complete::be_u64;

    // just error out if flags are never 0.

    dbg!(read);
    Ok(())
}
