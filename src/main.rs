use binrw::io::BufReader;
use binrw::BinRead;
use std::fs::File;
use std::io::{Cursor, Read};


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
