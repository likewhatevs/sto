use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct StackTrace {
    pub event: String,
    pub stack_node_datas: Vec<StackNodeData>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StoData {
    pub stack_nodes: HashMap<u64, StackNode>,
    pub stack_node_datas: HashMap<u64, StackNodeData>,
    pub profiled_binaries: HashMap<u64, ProfiledBinary>,
}

// perf script does not emit build id
// so this cli presumes all symbols are emitted for a single build id.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfiledBinary {
    pub id: u64,
    pub identifier: String,
    pub event: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StackNodeData {
    pub id: u64,
    pub symbol: String,
    pub file: String,
    pub bin_file: String,
    pub line_number: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StackNode {
    pub id: u64,
    pub parent_id: u64,
    pub data_id: u64,
    pub occurrences: u64,
    // kinda a big difference, but enables reconstruction w/o infra.
    pub depth: u32,
}
