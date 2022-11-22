use crate::globals::{BINARIES, DATAS, HASHER_SEED, NODES};
use crate::structs::{ProfiledBinary, StackNode, StackNodeData};
use cached::proc_macro::cached;
use cached::SizedCache;
use highway::{HighwayHash, HighwayHasher};
use lazy_static::lazy_static;
use regex::Regex;


lazy_static! {
    static ref HEADER_RE: Regex = Regex::new(
        r#"(?m)^.*\s+[0-9\-]+\s+\[[0-9]+\]\s+[0-9]+\.[0-9]+:\s+[0-9]+\s+(?P<event>.*?)[:].*$"#
    )
    .unwrap();
    static ref SYMBOL_RE: Regex =
        Regex::new(r#"(?m)^\s+[0-9a-f]+\s+((?P<symbol>.*)\+0x.*|(?P<other_sym>\[.*\]))\s\((?P<bin_file>.*)\).*$"#).unwrap();
    static ref FILE_LINE_NO_RE: Regex =
        Regex::new(r#"(?m)^\s+(((?P<src_file>.*?):(?P<line_no>[0-9]+))|\[(?P<src_file_2>\s*.*[0-9a-z\.]+)\]\[.*|((?P<src_file_3>.*[a-z0-9\.]+)\[.*))$"#).unwrap();
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
    let event: Option<String>;
    let mut symbol: Option<String> = None;
    let mut bin_file: Option<String> = None;
    let mut line_number: Option<u32> = None;
    let mut src_file: Option<String> = None;
    let mut depth: u32 = 0;
    let mut reset = false;
    fn clear(
        symbol: &mut Option<String>,
        bin_file: &mut Option<String>,
        line_number: &mut Option<u32>,
        src_file: &mut Option<String>,
    ) {
        *symbol = None;
        *bin_file = None;
        *line_number = None;
        *src_file = None;
    }
    if !data.is_empty() {
        event = HEADER_RE
            .captures(data.get(0).unwrap())
            .ok_or_else(|| log::error!("{:#?}", data.get(0)))
            .ok()
            .unwrap()
            .name("event")
            .map(|x| x.as_str().into());
        let profiled_binary = ProfiledBinary {
            id: root_id,
            identifier: identifier.clone(),
            event: event.unwrap(),
        };
        BINARIES
            .clone()
            .entry(profiled_binary.id)
            .or_insert(profiled_binary);
    }
    let mut parent_id = 0;
    let mut data = data;
    data.reverse();
    let mut it = data.iter().peekable();
    while let Some(row) = it.next() {
        if it.peek().is_some() {
            let sym_data = SYMBOL_RE.captures(row);

            if let Some(sym_data) = sym_data {
                symbol = sym_data
                    .name("symbol")
                    .map(|x| x.as_str().into())
                    .or_else(|| sym_data.name("other_sym").map(|x| x.as_str().into()));
                bin_file = sym_data.name("bin_file").map(|x| x.as_str().into());
            }

            let line_data = FILE_LINE_NO_RE.captures(row);
            if let Some(line_data) = line_data {
                src_file = line_data
                    .name("src_file")
                    .map(|x| x.as_str().into())
                    .or_else(|| line_data.name("src_file_2").map(|x| x.as_str().into()))
                    .or_else(|| line_data.name("src_file_3").map(|x| x.as_str().into()));
                line_number = line_data
                    .name("line_no")
                    .and_then(|x| x.as_str().parse().ok())
                    .or(Some(0));
            }
            if let Some(symbol) = symbol.clone() {
                if let Some(src_file) = src_file.clone() {
                    let data_id =
                        get_data_id(Some(symbol.clone()), Some(src_file.clone()), line_number);
                    let node_id = get_node_id(parent_id, data_id, root_id);
                    let stack_node_data = StackNodeData {
                        id: data_id,
                        symbol: symbol.clone(),
                        file: src_file,
                        line_number: line_number.unwrap_or(0),
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
                    reset = true;
                    depth += 1;
                    parent_id = node_id;
                }
            }
            if reset {
                clear(&mut symbol, &mut bin_file, &mut line_number, &mut src_file);
                reset = false;
            }
        }
    }
}
