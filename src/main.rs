#![feature(is_some_and)]

mod globals;
mod parse;
mod read;
mod structs;
mod unparse;
mod write;

use crate::read::{read_perf, read_sto};
use crate::unparse::{construct_template_data, unparse_and_write};
use crate::write::write_sto;
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
        help = "if present, unsto perf data. if absent, make sto data from perf data."
    )]
    unsto: bool,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    pretty_env_logger::init();

    let cli = Cli::parse();

    let in_file = cli.input_file;
    let out_file = cli.output_file;
    let unsto = cli.unsto;
    let binary_identifier = cli.binary_identifier;
    if !unsto {
        read_perf(in_file, binary_identifier).await?;
        write_sto(out_file).await?;
    } else {
        let sto = read_sto(in_file).await?;
        let template_data = construct_template_data(sto)?;
        unparse_and_write(template_data, out_file)?;
    }

    Ok(())
}
