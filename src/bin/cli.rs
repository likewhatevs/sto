use anyhow::{bail, Result};
use atomic_counter::AtomicCounter;
use blazesym::{BlazeSymbolizer, SymbolSrcCfg, SymbolizedResult, SymbolizerFeature};

use clap::Parser;
use core::time::Duration;
use deadqueue::limited::Queue;
use deepsize::DeepSizeOf;

use highway::{HighwayHash, HighwayHasher};
use libbpf_rs::RingBufferBuilder;
use perf_event_open_sys as perf;
use std::cmp::min;
use std::collections::HashMap;
use std::default::Default;
use std::future::Future;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender, sync_channel, SyncSender};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{process, thread, time};
use dotenvy::dotenv;
use sto::bpftune::bpftune_bss_types::stacktrace_event;
use sto::defs::{
    Args, EventType, ProcessQueue, ProfiledBinary, ReadQueue, StackInfo, StackNode, StackNodeData,
    StoData, HASHER_SEED, PROCESS_TASK_COUNT, READ_TASK_COUNT, WORKER_COUNT,
};
extern crate clap;
extern crate num_cpus;
use libbpf_rs::libbpf_sys::pid_t;
use tracing::{event, span, Level};
use moka::sync::Cache;
use once_cell::sync::Lazy;
use perf::perf_event_open;

use rlimit::Resource;

use rocket::form::validate::Len;

use sto::bpftune::*;
use symbolic_demangle::{Demangle, DemangleOptions};
use tracing_subscriber::Layer;

static SYM_CACHE: Lazy<Cache<String, String, ahash::RandomState>> = Lazy::new(|| {
    Cache::builder()
        .weigher(|key: &String, _value: &String| -> u32 {
            key.len().try_into().unwrap_or(u32::MAX)
        })
        .max_capacity(32 * 1024 * 1024)
        .build_with_hasher(ahash::RandomState::default())
});

static DATA_ID_CACHE: Lazy<Cache<StackNodeData, i64, ahash::RandomState>> = Lazy::new(|| {
    Cache::builder()
        .weigher(|key: &StackNodeData, _value: &i64| -> u32 {
            key.symbol.len().try_into().unwrap_or(u32::MAX)
                + key.file.len().try_into().unwrap_or(u32::MAX)
        })
        .max_capacity(32 * 1024 * 1024)
        .build_with_hasher(ahash::RandomState::default())
});

static NODE_ID_CACHE: Lazy<Cache<StackNode, i64, ahash::RandomState>> = Lazy::new(|| {
    Cache::builder()
        .max_capacity(8 * 1024 * 1024)
        .build_with_hasher(ahash::RandomState::default())
});

static MISC_ID_CACHE: Lazy<Cache<String, i64, ahash::RandomState>> = Lazy::new(|| {
    Cache::builder()
        .weigher(|key: &String, _value: &i64| -> u32 { key.len().try_into().unwrap_or(u32::MAX) })
        .max_capacity(2 * 1024 * 1024)
        .build_with_hasher(ahash::RandomState::default())
});


fn bump_memlock_rlimit() -> Result<()> {
    let (ml_soft, ml_hard) = Resource::get(rlimit::Resource::MEMLOCK)?;
    if min(ml_soft, ml_hard) < 128 << 20 {
        match Resource::set(Resource::MEMLOCK, 128 << 20, 128 << 20) {
            Ok(_x) => {
                event!(Level::INFO,"raised ulimit.");
            }
            Err(_x) => {
                bail!(
                    "unable to raise memlock limit and memlock limit uncomfortably low. \
                       please run the following command and retry:\n\
                       ulimit -l 134217728\n\
                       if that fails (probably will), follow these instructions: \n\
                       https://unix.stackexchange.com/a/359418 and retry that.\n\
                       *alternatively*, just re-run this with sudo"
                );
            }
        }
    }
    Ok(())
}

fn profile(args: Args, tx: Sender<StackInfo>) -> Result<()> {
    event!(Level::DEBUG,"IN PROFILE");
    let skel_builder = BpftuneSkelBuilder::default();
    bump_memlock_rlimit()?;
    let skel_ = skel_builder.open()?;
    let mut skel = skel_.load()?;
    let mut rbb = RingBufferBuilder::new();
    // https://github.com/rust-lang/rfcs/issues/2407
    let srsly_still_a_thing = args.clone();
    rbb.add(skel.maps_mut().events(), move |data: &[u8]| {
        let mut event = stacktrace_event::default();
        plain::copy_from_bytes(&mut event, data).expect("Event data buffer was too short");
        if event.pid == 0 {
            return 0;
        }
        let guess_it_is = srsly_still_a_thing.clone();
        let tx = tx.clone();
        thread::spawn(move || {
            tx.send(StackInfo {
                event,
                args: guess_it_is.clone(),
            }).unwrap()
        });
        0
    }).expect("error on callback on map");

    let rb = rbb.build()?;
    event!(Level::DEBUG,"CREATED RING BUFFER");

    let mut perf_fds = HashMap::new();

    for cpu in 0..num_cpus::get() {
        let mut attrs = perf::bindings::perf_event_attr::default();
        attrs.size = std::mem::size_of::<perf::bindings::perf_event_attr>() as u32;
        match args.event_type {
            EventType::Cycles => {
                attrs.type_ = perf::bindings::PERF_TYPE_HARDWARE;
                attrs.config = perf::bindings::PERF_COUNT_HW_CPU_CYCLES as u64;
            }
            EventType::Clock => {
                attrs.type_ = perf::bindings::PERF_TYPE_SOFTWARE;
                attrs.config = perf::bindings::PERF_COUNT_SW_CPU_CLOCK as u64;
            }
        }

        attrs.__bindgen_anon_1.sample_freq = args.sample_freq;
        attrs.set_freq(1);
        // attrs.set_exclude_kernel(0);
        attrs.set_exclude_hv(1);
        let result = unsafe {
            perf_event_open(
                &mut attrs,
                args.pid as pid_t,
                cpu as i32,
                -1,
                perf::bindings::PERF_FLAG_FD_CLOEXEC as u64,
            )
        };
        event!(Level::DEBUG,"attaching to perf event");
        let link = skel.progs_mut().profile().attach_perf_event(result)?;
        event!(Level::DEBUG,"attached to perf event");
        perf_fds.insert(result, link);
    }

    loop {
        rb.poll(Duration::from_millis(1))?;
        thread::sleep(Duration::from_secs(5));
    }

    event!(Level::DEBUG,"DONE ONE RUN");
    perf_fds.capacity();
    Ok(())
}

fn cached_demangle(mangled: &str) -> String {
    match SYM_CACHE.get(mangled) {
        Some(hit) => hit,
        None => {
            let name = symbolic_common::Name::from(mangled);
            let demangled = name.try_demangle(DemangleOptions::name_only());
            SYM_CACHE.insert(mangled.into(), demangled.clone().into());
            demangled.into()
        }
    }
}

fn misc_id(data: String) -> i64 {
    match MISC_ID_CACHE.get(&data) {
        Some(x) => x,
        None => {
            let mut hasher = HighwayHasher::new(HASHER_SEED);
            hasher.append(data.as_bytes());
            let id_neg: i64 = hasher.finalize64() as i64;
            let id = id_neg.abs() as i64;
            MISC_ID_CACHE.insert(data.clone(), id);
            id
        }
    }
}

fn id_stack_node(data: &mut StackNode) {
    let id = match NODE_ID_CACHE.get(data) {
        Some(hit) => hit,
        None => {
            let mut hasher = HighwayHasher::new(HASHER_SEED);
            if let Some(parent_id) = data.parent_id {
                hasher.append(&parent_id.to_be_bytes());
            }
            hasher.append(&data.stack_node_data_id.to_be_bytes());
            hasher.append(&data.profiled_binary_id.to_be_bytes());
            let id_neg: i64 = hasher.finalize64() as i64;
            let id = id_neg.abs() as i64;
            // should probably restructure this a bit because of 0 id in cache.
            NODE_ID_CACHE.insert(data.clone(), id);
            id
        }
    };
    data.id = id;
}

fn id_data(data: &mut StackNodeData) {
    let id = match DATA_ID_CACHE.get(data) {
        Some(hit) => hit,
        None => {
            let mut hasher = HighwayHasher::new(HASHER_SEED);
            hasher.append(data.symbol.as_bytes());
            if let Some(file) = data.clone().file {
                hasher.append(file.as_bytes());
            }
            if let Some(line_number) = data.line_number {
                hasher.append(&line_number.to_be_bytes());
            }
            let id_neg: i64 = hasher.finalize64() as i64;
            let id = id_neg.abs() as i64;
            // should probably restructure this a bit because of 0 id in cache.
            DATA_ID_CACHE.insert(data.clone(), id);
            id
        }
    };
    data.id = id;
}

fn symbolize(stack_info: StackInfo) -> Vec<Vec<SymbolizedResult>> {
    event!(Level::DEBUG,"IN SYMBOLIZE");
    let sym_srcs = [SymbolSrcCfg::Process {
        pid: Some(stack_info.args.pid),
    }];
    let symbolizer = BlazeSymbolizer::new_opt(&[SymbolizerFeature::LineNumberInfo(true)]).unwrap();
    let symlist = symbolizer.symbolize(&sym_srcs, stack_info.event.ustack.as_ref());
    symlist
}

fn process(args: Args) -> Result<(), anyhow::Error> {
    event!(Level::DEBUG,"IN PROCESS");
    let (tx, rx) = channel();
    let i_args = args.clone();
    let b_args = args.clone();
    thread::spawn(move || {
        let ii_args = i_args.clone();
        let mut buf = Vec::new();
        loop {
            let iii_args = ii_args.clone();
            match rx.recv() {
                Ok(data_chunk) => {
                    event!(Level::DEBUG,"READ DATA, BUF LEN:{}", buf.len().clone());
                    if(buf.len() < 200){
                        buf.push(symbolize(data_chunk).to_owned());
                    } else {
                        let old_buf = buf.to_owned();
                        thread::spawn(move || {
                            let iiii_args = iii_args.clone();
                            process_and_sink_data(old_buf.clone(), iiii_args.clone());
                            event!(Level::INFO,"SANK DATA");
                        });
                        buf.clear();
                    }
                }
                Err(_) => {
                }
            }
        }
    });


    profile(b_args.clone(), tx).expect("TODO: panic message");

    // done.
    Ok(())
}

fn process_and_sink_data(
    mut symlists: Vec<Vec<Vec<SymbolizedResult>>>,
    args: Args,
) -> Result<(), anyhow::Error> {
        event!(Level::DEBUG,"stack is");
        let mut stack_node_map: HashMap<i64, StackNode> = HashMap::new();
        let mut stack_node_data_map: HashMap<i64, StackNodeData> = HashMap::new();
        let mut profiled_binary_map: HashMap<i64, ProfiledBinary> = HashMap::new();
        let basename = args.clone().binary.clone();
        let version = args.clone().version;
        let id = match version {
            Some(x) => {
                misc_id(format!("{}{}", args.clone().binary.unwrap().clone(), x))
            },
            None => {
                misc_id(args.clone().binary.unwrap().clone())
            }
        };

        let profiled_binary = ProfiledBinary {
            id,
            event: args.event_type.to_string(),
            build_id: args.version,
            basename: basename.unwrap(),
            updated_at: None,
            created_at: None,
            sample_count: 0,
            raw_data_size: 0,
            processed_data_size: 0,
        };

        let _cur_bin_id = profiled_binary.id;
    for mut symlist in symlists {
        let mut parent_id: Option<i64> = None;
        symlist.reverse();
        for mut stack in symlist {
            profiled_binary_map
                .entry(profiled_binary.id)
                .or_insert(profiled_binary.clone());
            profiled_binary_map
                .entry(profiled_binary.id)
                .and_modify(|e| e.sample_count += 1)
                .and_modify(|e| e.raw_data_size += stack.deep_size_of() as i64);
            // stack.reverse();
            for (_i, frame) in stack.iter().enumerate() {
                let mut data = StackNodeData {
                    id: 0,
                    symbol: cached_demangle(&frame.symbol).to_string(),
                    file: if frame.path.trim().is_empty() {
                        None
                    } else {
                        Some(frame.path.trim().into())
                    },
                    line_number: if frame.line_no > 0 {
                        Some(frame.line_no as i32)
                    } else {
                        None
                    },
                };
                id_data(&mut data);
                stack_node_data_map.entry(data.id).or_insert(data.clone());
                let mut stack_node = StackNode {
                    id: 0,
                    parent_id,
                    stack_node_data_id: data.id,
                    profiled_binary_id: profiled_binary.id,
                    sample_count: 1,
                };
                id_stack_node(&mut stack_node);
                stack_node_map
                    .entry(stack_node.id)
                    .and_modify(|e| e.sample_count += 1)
                    .or_insert(stack_node.clone());
                parent_id = Some(stack_node.id);
            }
        }
    }

        let mut data_out = StoData {
            stack_nodes: stack_node_map.values().map(|x| (*x).clone()).collect(),
            stack_node_datas: stack_node_data_map.values().map(|x| (*x).clone()).collect(),
            profiled_binaries: profiled_binary_map.values().map(|x| (*x).clone()).collect(),
        };

        profiled_binary_map
            .entry(profiled_binary.id)
            .and_modify(|e| e.processed_data_size += data_out.deep_size_of() as i64);

        data_out.profiled_binaries = profiled_binary_map.values().map(|x| (*x).clone()).collect();

        let client = reqwest::blocking::Client::new();
        match client.post(args.url.clone()).json(&data_out).send() {
            Ok(x) => match x.error_for_status() {
                Ok(_x) => {}
                Err(x) => {
                    event!(Level::ERROR,"failed to post data: {}", x);
                }
            },
            Err(x) => {
                event!(Level::ERROR,"failed to post data: {}", x);
            }
        }

    Ok(())
}


fn main() -> Result<(), anyhow::Error> {
    dotenvy::dotenv()?;
    use tracing_subscriber::prelude::*;
    let console_layer = console_subscriber::spawn();
    tracing_subscriber::registry()
        .with(console_layer)
        .with(tracing_subscriber::fmt::layer()
            .with_level(true)
            .with_line_number(true)
            .with_thread_names(true)
            .with_filter(tracing_subscriber::filter::LevelFilter::from_level(Level::DEBUG)))
        .init();

    let mut args = Args::parse();
    if args.binary.is_none(){
        args.binary = Some("provide_a_meaningful_name".to_string());
    }
    if args.pid == 0 {
        event!(Level::ERROR, "please provide a pid");
        std::process::exit(-1);
    }


    process(args.clone());

    Ok(())
}
