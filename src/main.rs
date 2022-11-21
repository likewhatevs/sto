#![feature(is_some_and)]

mod globals;
mod parse;
mod read;
mod structs;
mod unparse;
mod write;

use crate::read::read;
use crate::write::write;
use clap::Parser;
use log::error;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about,
    long_about = "sto and unsto perf dumps. only supports single binary/event type dumps for now."
)]
struct Cli {
    #[arg(short, long, required = true)]
    input_file: PathBuf,

    #[arg(short, long, required = true)]
    output_file: PathBuf,

    #[arg(short, long, default_value_t = String::from("binary identifier"), help = "thing to uniquely identify binary in question")]
    binary_identifier: String,

    #[arg(
        short,
        long,
        help = "if true, sto perf data. if false, unsto sto data into perf.",
        required = true
    )]
    sto: bool,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    pretty_env_logger::init();

    let cli = Cli::parse();

    let in_file = cli.input_file;
    let out_file = cli.output_file;
    let mode = cli.sto;
    let binary_identifier = cli.binary_identifier;
    if mode {
        read(in_file, binary_identifier).await?;
        write().await?;
    } else {
    }

    Ok(())
}
