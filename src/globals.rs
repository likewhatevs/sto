use crate::structs::{ProfiledBinary, StackNode, StackNodeData};
use dashmap::DashMap;
use highway::Key;
use once_cell::sync::Lazy;
use std::sync::Arc;

pub const TASK_COUNT: usize = 1000;
pub const WORKER_COUNT: usize = 100;
pub type TaskQueue = deadqueue::limited::Queue<Vec<String>>;
pub const HASHER_SEED: Key = Key([1, 2, 3, 4]);
pub static NODES: Lazy<Arc<DashMap<u64, StackNode>>> = Lazy::new(|| Arc::new(DashMap::new()));
pub static DATAS: Lazy<Arc<DashMap<u64, StackNodeData>>> = Lazy::new(|| Arc::new(DashMap::new()));
pub static BINARIES: Lazy<Arc<DashMap<u64, ProfiledBinary>>> =
    Lazy::new(|| Arc::new(DashMap::new()));
