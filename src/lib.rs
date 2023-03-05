use std::num::ParseIntError;
use std::str::FromStr;
use plain::Plain;
use crate::bpftune::bpftune_bss_types;

pub mod defs;
#[path = "bpf/bpftune.skel.rs"]
pub mod bpftune;

unsafe impl Plain for bpftune_bss_types::stacktrace_event {}

impl FromStr for bpftune_bss_types::stacktrace_event {
    type Err = ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let split: Vec<&str> = s.split_whitespace().collect();
        let mut event = bpftune_bss_types::stacktrace_event::default();
        event.pid = split[2].parse().unwrap();
        let ustack: [u64; 128] = [split[4].parse().unwrap(); 128];
        event.ustack = ustack;
        let kstack: [u64; 128] = [split[6].parse().unwrap(); 128];
        event.kstack = kstack;
        Ok(event)
    }
}