#![feature(is_some_and)]

use std::collections::HashMap;
use tokio::fs::File;
use std::sync::Arc;
use highway::{HighwayHash, HighwayHasher, Key};
use lazy_static::lazy_static;
use tokio::io::{AsyncBufReadExt, AsyncSeek, AsyncWriteExt, BufReader, BufWriter};
use tokio::time;
use tokio::time::{sleep, Duration};
use regex::Regex;
use cached::proc_macro::cached;
use cached::SizedCache;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde_derive::{Serialize,Deserialize};
use rmp_serde as rmps;
use rmps::{Deserializer, Serializer};
use serde::Serialize;

const TASK_COUNT: usize = 1000;
const WORKER_COUNT: usize = 10;

type TaskQueue = deadqueue::limited::Queue<Vec<String>>;

const HASHER_SEED: Key = Key([1, 2, 3, 4]);

static NODES: Lazy<Arc<DashMap<u64,StackNode>>> = Lazy::new(||{
                    Arc::new(DashMap::new())
                });
static DATAS: Lazy<Arc<DashMap<u64,StackNodeData>>> = Lazy::new(||{
    Arc::new(DashMap::new())
});
static BINARIES: Lazy<Arc<DashMap<u64,ProfiledBinary>>> = Lazy::new(||{
    Arc::new(DashMap::new())
});

lazy_static! {
    static ref HEADER_RE: Regex = Regex::new(r"^.*\s+[0-9]+\s+\[[0-9]+\]\s+[0-9]+\.[0-9]+:\s+[0-9]+\s+(?P<event>.*?)[:].*$").unwrap();
    static ref SYMBOL_RE: Regex = Regex::new(r"^\s+[0-9a-f]+\s+(?P<symbol>.*)\s\((?P<bin_file>.*)\).*$").unwrap();
    static ref FILE_LINE_NO_RE: Regex = Regex::new(r"^(?P<src_file>\s+.*?):(?P<line_no>[0-9]+)$").unwrap();
}

#[derive(Debug)]
pub struct StackTrace {
    pub event: String,
    pub stack_ndoe_datas: Vec<StackNodeData>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StoData{
    pub stack_nodes: HashMap<u64, StackNode>,
    pub stack_node_datas: HashMap<u64, StackNodeData>,
    pub profiled_binaries: HashMap<u64, ProfiledBinary>,
}

// perf script does not emit build id
// so this cli presumes all symbols are emitted for a single build id.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfiledBinary{
    pub id: u64,
    pub identifier: String,
    pub event: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StackNodeData{
    pub id: u64,
    pub symbol: String,
    pub file: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StackNode{
    pub id: u64,
    pub parent_id: u64,
    pub data_id: u64,
    pub occurrences: u64,
    // kinda a big difference, but enables reconstruction w/o infra.
    pub depth: u32,
}

#[cached(type = "SizedCache<(u64,u64,u64), u64>",
    create = "{ SizedCache::with_size(10000) }",
    convert = r#"{ (parent_id, data_id, root_id) }"#)]
fn get_node_id(parent_id: u64, data_id: u64, root_id: u64) -> u64{
    let d_str = format!("{:?}{:?}{:?}", parent_id, data_id, root_id);
    let mut hasher = HighwayHasher::new(HASHER_SEED);
    hasher.append(d_str.as_bytes());
    let id: u64 = hasher.finalize64();
    id >> 1
}

#[cached(type = "SizedCache<String, u64>",
create = "{ SizedCache::with_size(10000) }",
convert = r#"{ format!("{:?}{:?}{:?}", symbol, file, line_number) }"#)]
fn get_data_id(symbol: Option<String>, file: Option<String>, line_number: Option<u32>) -> u64{
    let d_str = format!("{:?}{:?}{:?}", symbol, file, line_number);
    let mut hasher = HighwayHasher::new(HASHER_SEED);
    hasher.append(d_str.as_bytes());
    let id: u64 = hasher.finalize64();
    id >> 1
}

async fn process_record(data: Vec<String>, root_id: u64, identifier: String){
    let mut event: Option<String> = Option::None;
    let mut symbol: Option<String> = Option::None;
    let mut bin_file: Option<String> = Option::None;
    let mut file: Option<String> = Option::None;
    let mut line_number: Option<u32> = Option::None;
    let mut src_file: Option<String> = Option::None;
    let mut depth: u32 = 0;
    fn clear(mut symbol: Option<String>, mut bin_file: Option<String>, mut file: Option<String>, mut line_number: Option<u32>, mut src_file: Option<String>){
        symbol = Option::None;
        bin_file = Option::None;
        file = Option::None;
        line_number = Option::None;
        src_file = Option::None;
    }
    let mut is_symbol_line = false;
    if data.len()>0 {
        event = HEADER_RE.captures(data.get(0).unwrap()).unwrap().name("event").map(|x| x.as_str().into());
        let profiled_binary = ProfiledBinary{
            id: root_id,
            identifier,
            event: event.unwrap(),
        };
        BINARIES.clone().entry(profiled_binary.id).or_insert(profiled_binary);
    }
    let mut parent_id = 0;
    let mut data = data.clone();
    data.reverse();
    let mut it = data.iter().peekable();
    while let Some(row) = it.next()  {
        if !it.peek().is_none() {
            if is_symbol_line {
                let sym_data = SYMBOL_RE.captures(&row).unwrap();
                symbol = sym_data.name("symbol").map(|x| x.as_str().into());
                bin_file = sym_data.name("bin_file").map(|x| x.as_str().into());
                is_symbol_line = false;
                file = src_file.clone().or_else(|| bin_file.clone());
                let data_id = get_data_id(symbol.clone(), file.clone(), line_number.clone());
                let node_id = get_node_id(parent_id.clone(), data_id.clone(), root_id);
                let stack_node_data = StackNodeData{
                    id: data_id,
                    symbol: symbol.clone().unwrap_or("".into()),
                    file: file.clone().unwrap_or("".into())
                };
                let stack_node = StackNode{
                    id: node_id,
                    parent_id,
                    data_id,
                    occurrences: 1,
                    depth
                };
                NODES.clone().entry(stack_node.id).and_modify(|x| x.occurrences += 1).or_insert(stack_node);
                DATAS.clone().entry(stack_node_data.id).or_insert(stack_node_data);
                clear(symbol.clone(),
                      bin_file.clone(),
                      file.clone(),
                      line_number,
                      src_file.clone());
                depth += 1;
                parent_id = node_id;
            } else {
                let line_data = FILE_LINE_NO_RE.captures(&row).unwrap();
                src_file = line_data.name("src_file").map(|x| x.as_str().into());
                line_number = line_data.name("line_no").map(|x| x.as_str().parse().ok()).flatten().or_else(|| Some(0));
                is_symbol_line = true;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    pretty_env_logger::init();
    let path = "/Users/patsomaru/Documents/GitHub/sto/tests/cpu_data.txt";
    let file = File::open(path).await?;
    let binary_identifier = "thing you pass in sry";
    let mut hasher = HighwayHasher::new(HASHER_SEED);
    hasher.append(binary_identifier.as_bytes());
    let root_id = hasher.finalize64() >> 1;
    let queue = Arc::new(TaskQueue::new(TASK_COUNT));
    let buf_reader = BufReader::new(file);
    let mut lines = buf_reader.lines();
    let mut buf: Vec<String> = Vec::new();
    let queue = queue.clone();

    for _a in 1..WORKER_COUNT{
        let q_ref = queue.clone();
        tokio::spawn(
                async move {
                    loop {
                        let data_chunk = q_ref.pop().await;
                        process_record(data_chunk, root_id, binary_identifier.into()).await;
                    }
                }
            );
    }

    while let Some(line) = lines.next_line().await? {
        if line.is_empty() {
            let mut done= false;
            while !done {
                match queue.try_push(buf.clone()) {
                    Err(x) => (),
                    Ok(x) => {
                        done=true;
                        ()
                    }
                }
                // noop 10 ms sleep.
                let _ = time::sleep(Duration::from_millis(10));
            }
            buf.clear();
        } else {
            buf.push(format!("{}\n", line).into());
        }
    }

    while !queue.is_empty() {
        let _ = time::sleep(Duration::from_millis(10));
    }

    let mut data_out = StoData{
        stack_node_datas: HashMap::from_iter(DATAS.clone().iter().map(|x| (x.key().clone(),x.value().clone()))),
        stack_nodes: HashMap::from_iter(NODES.clone().iter().map(|x| (x.key().clone(),x.value().clone()))),
        profiled_binaries: HashMap::from_iter(BINARIES.clone().iter().map(|x| (x.key().clone(),x.value().clone()))),
    };


    let mut outbuf = Vec::new();
    data_out.serialize(&mut Serializer::new(&mut outbuf)).unwrap();
    let mut outfile = File::open("outfile").await?;
    let mut bufwriter = BufWriter::new(outfile);
    bufwriter.write_all(&mut outbuf).await?;
    bufwriter.flush().await?;
    bufwriter.shutdown().await?;
    // just error out if flags are never 0.

    Ok(())
}
