#![feature(is_some_and)]

mod globals;
mod parse;
mod read;
mod structs;
mod unparse;
mod write;

use crate::read::read;
use crate::write::write;
use highway::{HighwayHash, HighwayHasher, Key};
use rmp_serde as rmps;
use rmps::{Deserializer, Serializer};
use structs::{ProfiledBinary, StackNode, StackNodeData, StoData};
use tokio::io::{AsyncBufReadExt, AsyncSeek, AsyncWriteExt, BufReader, BufWriter};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    pretty_env_logger::init();

    read().await?;

    write().await?;

    Ok(())
}
