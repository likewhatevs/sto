use crate::globals::{BINARIES, DATAS, HASHER_SEED, NODES};
use crate::structs::{ProfiledBinary, StackNode, StackNodeData};
use cached::proc_macro::cached;
use cached::SizedCache;
use highway::{HighwayHash, HighwayHasher};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // borrowed some from
    // https://github.com/spiermar/burn/blob/master/convert/perf.go
    // thx!
    static ref HEADER_EVENT_RE: Regex = Regex::new(
        r#"^\s+(?P<weight>[0-9]+)(?P<event>\s[a-zA-Z\-_:]+):$"#
    )
    .unwrap();
    static ref SYMBOL_RE: Regex =
        Regex::new(r#"^\s+[0-9a-f]+\s(?P<symbol>.*)$"#).unwrap();
    // this mess is all me, works well enough.
    static ref FILE_LINE_NO_RE: Regex =
        Regex::new(r#"^\s+(?P<src_file>\/.*):(?P<line_number>[0-9]+)$"#).unwrap();
    static ref END_RE: Regex = Regex::new(r#"^$"#).unwrap();
}

enum State {
    Header,
    Symbol,
    LineNumber,
    Unknown,
    End,
}

#[cached(
    type = "SizedCache<(u64,u64,u64), u64>",
    create = "{ SizedCache::with_size(10000) }",
    convert = r#"{ (parent_id, data_id, root_id) }"#
)]
fn get_node_id(parent_id: u64, data_id: u64, root_id: u64) -> u64 {
    let d_str = format!("{parent_id:?}{data_id:?}{root_id:?}");
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
fn get_data_id(
    symbol: Option<String>,
    file: Option<String>,
    line_number: Option<u32>,
) -> u64{
    let d_str = format!("{symbol:?}{file:?}{line_number:?}");
    let mut hasher = HighwayHasher::new(HASHER_SEED);
    hasher.append(d_str.as_bytes());
    let id: u64 = hasher.finalize64();
    id >> 1
}

#[derive(Default, Debug, Clone)]
struct RawData {
    src_file: Option<String>,
    line_number: Option<u32>,
    symbol: Option<String>,
}

pub async fn process_record(data: Vec<String>, root_id: u64, identifier: String) {
    let mut this_raw_data = RawData {
        ..Default::default()
    };
    let mut reversed_stack: Vec<RawData> = Vec::new();
    let mut state = State::Header;
    let mut cur_weight = 0;
    let it = data.iter().peekable();
    for row in it {
        let mut process_line = true;
        while process_line {
            process_line = false;
            match state {
                State::Header => {
                    reversed_stack.clear();
                    if BINARIES.clone().is_empty() {
                        let caps = HEADER_EVENT_RE
                            .captures(row)
                            .ok_or_else(|| log::error!("header {:#?}", row))
                            .ok();
                        if let Some(capture) = caps {
                            let event = capture.name("event").map(|x| x.as_str().into());
                            let weight: String = capture.name("weight").map(|x| x.as_str().into()).unwrap();
                            let weight_int: u64 = weight.parse().unwrap();
                            cur_weight = weight_int;
                            let pb = ProfiledBinary {
                                id: root_id,
                                identifier: identifier.clone(),
                                event: event.unwrap(),
                                total_samples: weight_int,
                            };
                            BINARIES.clone().entry(pb.id).or_insert(pb.clone());
                        }
                    } 
                    state = State::Symbol;
                }
                State::Symbol => {
                    let caps = SYMBOL_RE
                        .captures(row)
                        .ok_or_else(|| log::warn!("sym {:#?}", row))
                        .ok();
                    if let Some(captured) = caps {
                        this_raw_data.symbol = captured.name("symbol").map(|x| x.as_str().into());
                    }
                    state = State::LineNumber;
                }
                State::LineNumber => {
                    // this doesn't need to regex twice..
                    let caps = FILE_LINE_NO_RE
                        .captures(row)
                        .ok_or_else(|| log::debug!("file {:#?}", row))
                        .ok();
                    if let Some(captured) = caps {
                        this_raw_data.line_number = captured
                            .name("line_number")
                            .and_then(|x| x.as_str().parse().ok());
                        this_raw_data.src_file =
                            captured.name("src_file").map(|x| x.as_str().into());
                    }
                    state = State::Unknown;
                }
                State::Unknown => {
                    reversed_stack.push(this_raw_data.clone());
                    this_raw_data = RawData {
                        ..Default::default()
                    };
                    process_line = true;
                    if END_RE.is_match(row) {
                        state = State::End;
                    } else {
                        state = State::Symbol;
                    }
                }
                State::End => {
                    reversed_stack.reverse();
                    let mut parent_id = 0;
                    let mut tmp_list_node = Vec::new();
                    let mut tmp_list_data = Vec::new();
                    for (depth, i) in reversed_stack.clone().into_iter().enumerate() {
                        if i.symbol.is_none() {
                            tmp_list_node.clear();
                            tmp_list_data.clear();
                            break;
                        }
                        let data_id = get_data_id(
                            i.symbol.clone(),
                            i.src_file.clone(),
                            i.line_number,
                        );
                        let node_id = get_node_id(parent_id, data_id, root_id);
                        let stack_node_data = StackNodeData {
                            id: data_id,
                            // sus.
                            symbol: i.symbol.clone().unwrap(),
                            file: i.src_file.clone().unwrap_or("".into()),
                            line_number: i.line_number.unwrap_or(0),
                        };
                        let stack_node = StackNode {
                            id: node_id,
                            parent_id,
                            data_id,
                            occurrences: cur_weight,
                            depth: u32::try_from(depth).unwrap(),
                        };
                        tmp_list_data.push(stack_node_data);
                        tmp_list_node.push(stack_node);
                        parent_id = node_id;
                    }
                    for stack_node in tmp_list_node.clone(){
                        NODES
                            .clone()
                            .entry(stack_node.id)
                            .and_modify(|x| x.occurrences += 1)
                            .or_insert(stack_node);

                    }
                    for stack_node_data in tmp_list_data{
                        DATAS
                            .clone()
                            .entry(stack_node_data.id)
                            .or_insert(stack_node_data);
                    }
                    if !tmp_list_node.is_empty(){
                        BINARIES.clone().entry(root_id).and_modify(|x| x.total_samples += 1);
                    }
                    state = State::Header;
                }
            }
        }
    }
}
