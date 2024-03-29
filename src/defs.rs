use std::collections::HashMap;
use crate::bpftune::bpftune_bss_types::stacktrace_event;
use blazesym::SymbolizedResult;
use chrono::{DateTime, Utc};
use clap::{arg, command};
use clap::{Parser, ValueEnum};
use dashmap::DashMap;
use highway::Key;
use once_cell::sync::Lazy;
use serde_derive::{Deserialize, Serialize};
use sqlx::FromRow;

use std::sync::Arc;

#[macro_use]
use enum_display_derive;
use deepsize::DeepSizeOf;
use std::fmt::Display;

pub const READ_TASK_COUNT: usize = 10000000;
pub const PROCESS_TASK_COUNT: usize = 100;
pub const WORKER_COUNT: usize = 4;

pub type ReadQueue = deadqueue::limited::Queue<StackInfo>;
pub type ProcessQueue = deadqueue::limited::Queue<Vec<Vec<SymbolizedResult>>>;

pub const HASHER_SEED: Key = Key([1, 2, 3, 4]);
pub static NODES: Lazy<Arc<DashMap<i64, StackNode>>> = Lazy::new(|| Arc::new(DashMap::new()));
pub static DATAS: Lazy<Arc<DashMap<i64, StackNodeData>>> = Lazy::new(|| Arc::new(DashMap::new()));
pub static BINARIES: Lazy<Arc<DashMap<i64, Executable>>> =
    Lazy::new(|| Arc::new(DashMap::new()));

#[derive(ValueEnum, Debug, Serialize, Deserialize, Clone, Copy, enum_display_derive::Display)]
pub enum EventType {
    Cycles,
    Clock,
}

#[clap(disable_version_flag = true)]
#[derive(Parser, Debug, Serialize, Deserialize, Clone)]
#[command(author, version, about, long_about = "Do stuff")]
pub struct Args {
    #[arg(
        short,
        long,
        default_value_t = 0,
        help = "to profile a running process"
    )]
    pub pid: u32,
    #[arg(short, long, default_value_t = 100000)]
    pub total_samples: u32,
    #[arg(value_enum, short, long, default_value_t = EventType::Cycles)]
    pub event_type: EventType,
    #[arg(short, long, default_value_t = 100000, help = "sample frequency.")]
    pub sample_freq: u64,
    #[arg(short, long, help = "name of thing being profiled")]
    pub binary: Option<String>,
    #[arg(short, long, help = "version of thing being profiled")]
    pub version: Option<String>,
    #[arg(
        short,
        long,
        help = "write data to the specified url",
        default_value = "http://localhost:8000/data/samples"
    )]
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, DeepSizeOf)]
pub struct StoData {
    pub stack_nodes: Vec<StackNode>,
    pub stack_node_datas: Vec<StackNodeData>,
    pub profiled_binaries: Vec<Executable>,
}

#[derive(Debug, Clone)]
pub struct StackInfo {
    pub event: stacktrace_event,
    pub args: Args,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow, Hash, Eq, PartialEq, DeepSizeOf)]
pub struct Executable {
    pub id: i64,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
    pub basename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    pub sample_count: i64,
    pub raw_data_size: i64,
    pub processed_data_size: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow, Hash, Eq, PartialEq, DeepSizeOf)]
pub struct StackNode {
    pub id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<i64>,
    pub stack_node_data_id: i64,
    pub executable_id: i64,
    pub sample_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow, Hash, Eq, PartialEq, DeepSizeOf)]
pub struct StackNodeData {
    pub id: i64,
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_number: Option<i32>,
}

impl FromIterator<StackNodeData> for HashMap<i64, StackNodeData> {
    fn from_iter<I>(xs: I) -> Self
        where
            I: IntoIterator<Item = StackNodeData>,
    {
        xs.into_iter().map(|x| (x.id.clone(), x)).collect()
    }
}

impl FromIterator<Executable> for HashMap<i64, Executable> {
    fn from_iter<I>(xs: I) -> Self
        where
            I: IntoIterator<Item = Executable>,
    {
        xs.into_iter().map(|x| (x.id.clone(), x)).collect()
    }
}
