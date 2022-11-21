use crate::globals::{BINARIES, DATAS, HASHER_SEED, NODES};
use crate::structs::{ProfiledBinary, StackNode, StackNodeData, StackTrace};
use cached::proc_macro::cached;
use cached::SizedCache;
use highway::{HighwayHash, HighwayHasher, Key};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref HEADER_RE: Regex =
        Regex::new(r"^.*\s+[0-9]+\s+\[[0-9]+\]\s+[0-9]+\.[0-9]+:\s+[0-9]+\s+(?P<event>.*?)[:].*$")
            .unwrap();
    static ref SYMBOL_RE: Regex =
        Regex::new(r"^\s+[0-9a-f]+\s+(?P<symbol>.*)\s\((?P<bin_file>.*)\).*$").unwrap();
    static ref FILE_LINE_NO_RE: Regex =
        Regex::new(r"^(?P<src_file>\s+.*?):(?P<line_no>[0-9]+)$").unwrap();
}

#[cached(
    type = "SizedCache<(u64,u64,u64), u64>",
    create = "{ SizedCache::with_size(10000) }",
    convert = r#"{ (parent_id, data_id, root_id) }"#
)]
fn get_node_id(parent_id: u64, data_id: u64, root_id: u64) -> u64 {
    let d_str = format!("{:?}{:?}{:?}", parent_id, data_id, root_id);
    let mut hasher = HighwayHasher::new(HASHER_SEED);
    hasher.append(d_str.as_bytes());
    let id: u64 = hasher.finalize64();
    id >> 1
}

#[cached(
    type = "SizedCache<String, u64>",
    create = "{ SizedCache::with_size(10000) }",
    convert = r#"{ format!("{:?}{:?}{:?}", symbol, file, line_number) }"#
)]
fn get_data_id(symbol: Option<String>, file: Option<String>, line_number: Option<u32>) -> u64 {
    let d_str = format!("{:?}{:?}{:?}", symbol, file, line_number);
    let mut hasher = HighwayHasher::new(HASHER_SEED);
    hasher.append(d_str.as_bytes());
    let id: u64 = hasher.finalize64();
    id >> 1
}

pub async fn process_record(data: Vec<String>, root_id: u64, identifier: String) {
    let mut event: Option<String> = Option::None;
    let mut symbol: Option<String> = Option::None;
    let mut bin_file: Option<String> = Option::None;
    let mut file: Option<String> = Option::None;
    let mut line_number: Option<u32> = Option::None;
    let mut src_file: Option<String> = Option::None;
    let mut depth: u32 = 0;
    fn clear(
        mut symbol: Option<String>,
        mut bin_file: Option<String>,
        mut file: Option<String>,
        mut line_number: Option<u32>,
        mut src_file: Option<String>,
    ) {
        symbol = Option::None;
        bin_file = Option::None;
        file = Option::None;
        line_number = Option::None;
        src_file = Option::None;
    }
    let mut is_symbol_line = false;
    if data.len() > 0 {
        event = HEADER_RE
            .captures(data.get(0).unwrap())
            .unwrap()
            .name("event")
            .map(|x| x.as_str().into());
        let profiled_binary = ProfiledBinary {
            id: root_id,
            identifier,
            event: event.unwrap(),
        };
        BINARIES
            .clone()
            .entry(profiled_binary.id)
            .or_insert(profiled_binary);
    }
    let mut parent_id = 0;
    let mut data = data.clone();
    data.reverse();
    let mut it = data.iter().peekable();
    while let Some(row) = it.next() {
        if !it.peek().is_none() {
            if is_symbol_line {
                let sym_data = SYMBOL_RE.captures(&row).unwrap();
                symbol = sym_data.name("symbol").map(|x| x.as_str().into());
                bin_file = sym_data.name("bin_file").map(|x| x.as_str().into());
                is_symbol_line = false;
                file = src_file.clone().or_else(|| bin_file.clone());
                let data_id = get_data_id(symbol.clone(), file.clone(), line_number.clone());
                let node_id = get_node_id(parent_id.clone(), data_id.clone(), root_id);
                let stack_node_data = StackNodeData {
                    id: data_id,
                    symbol: symbol.clone().unwrap_or("".into()),
                    file: file.clone().unwrap_or("".into()),
                };
                let stack_node = StackNode {
                    id: node_id,
                    parent_id,
                    data_id,
                    occurrences: 1,
                    depth,
                };
                NODES
                    .clone()
                    .entry(stack_node.id)
                    .and_modify(|x| x.occurrences += 1)
                    .or_insert(stack_node);
                DATAS
                    .clone()
                    .entry(stack_node_data.id)
                    .or_insert(stack_node_data);
                clear(
                    symbol.clone(),
                    bin_file.clone(),
                    file.clone(),
                    line_number,
                    src_file.clone(),
                );
                depth += 1;
                parent_id = node_id;
            } else {
                let line_data = FILE_LINE_NO_RE.captures(&row).unwrap();
                src_file = line_data.name("src_file").map(|x| x.as_str().into());
                line_number = line_data
                    .name("line_no")
                    .and_then(|x| x.as_str().parse().ok())
                    .or(Some(0));
                is_symbol_line = true;
            }
        }
    }
}
