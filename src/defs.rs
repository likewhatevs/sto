use crate::bpftune::bpftune_bss_types::stacktrace_event;
use blazesym::SymbolizedResult;
use chrono::{DateTime, Utc};
use clap::{arg, command};
use clap::{Parser, ValueEnum};
use copystr::CopyStringCapacity;
use dashmap::DashMap;
use highway::Key;
use once_cell::sync::Lazy;
use serde_derive::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::Arc;

pub const READ_TASK_COUNT: usize = 10000000;
pub const PROCESS_TASK_COUNT: usize = 100;
pub const WORKER_COUNT: usize = 4;

pub type ReadQueue = deadqueue::limited::Queue<StackInfo>;
pub type ProcessQueue = deadqueue::limited::Queue<Vec<Vec<SymbolizedResult>>>;

pub const HASHER_SEED: Key = Key([1, 2, 3, 4]);
pub static NODES: Lazy<Arc<DashMap<u64, StackNode>>> = Lazy::new(|| Arc::new(DashMap::new()));
pub static DATAS: Lazy<Arc<DashMap<u64, StackNodeData>>> = Lazy::new(|| Arc::new(DashMap::new()));
pub static BINARIES: Lazy<Arc<DashMap<u64, ProfiledBinary>>> =
    Lazy::new(|| Arc::new(DashMap::new()));

#[derive(ValueEnum, Debug, Serialize, Deserialize, Clone, Copy)]
pub enum EventType {
    Cycles,
    Clock,
}

#[derive(Parser, Debug, Serialize, Deserialize, Clone, Copy)]
#[command(author, version, about, long_about = "Do stuff")]
pub struct Args {
    #[arg(short, long, required = true)]
    pub pid: u32,
    #[arg(short, long, default_value_t = 100000)]
    pub samples: u32,
    #[arg(value_enum, short, long, default_value_t = EventType::Cycles)]
    pub event_type: EventType,
    #[arg(short, long, default_value_t = 100000, help = "sample frequency.")]
    pub sample_freq: u64,
    #[arg(
        short,
        long,
        help = "if present, write parsed data to provided postgresql."
    )]
    pub db: CopyStringCapacity,
}

#[derive(Parser, Debug, Serialize, Deserialize, Clone)]
#[command(author, version, about, long_about = "Do *server* stuff")]
pub struct ServerArgs {
    #[arg(short, long, default_value_t = String::from("localhost"),  help = "Serve data from the specified postgresql")]
    pub db: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StoDataMaps {
    pub stack_nodes: Arc<DashMap<u64, StackNode>>,
    pub stack_node_datas: Arc<DashMap<u64, StackNodeData>>,
    pub profiled_binaries: Arc<DashMap<u64, ProfiledBinary>>,
}

#[derive(Debug, Clone)]
pub struct StackInfo {
    pub event: stacktrace_event,
    pub args: Args,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct ProfiledBinary {
    pub id: u64,
    pub event: String,
    pub build_id: String,
    pub basename: String,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub sample_count: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct StackNode {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub stack_node_data_id: u64,
    pub profiled_binary_id: u64,
    pub sample_count: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct StackNodeData {
    pub id: u64,
    pub symbol: String,
    pub file: String,
    pub line_number: u32,
}
