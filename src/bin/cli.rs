use anyhow::{bail, Result};
use async_ctrlc::CtrlC;
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
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{process, thread};
use sto::bpftune::bpftune_bss_types::stacktrace_event;
use sto::defs::{
    Args, EventType, ProcessQueue, ProfiledBinary, ReadQueue, StackInfo, StackNode, StackNodeData,
    StoData, HASHER_SEED, PROCESS_TASK_COUNT, READ_TASK_COUNT, WORKER_COUNT,
};
extern crate clap;
extern crate num_cpus;
use libbpf_rs::libbpf_sys::pid_t;

use log::{error, info};
use moka::sync::Cache;
use once_cell::sync::Lazy;
use perf::perf_event_open;

use rlimit::Resource;

use rocket::form::validate::Len;

use sto::bpftune::*;
use symbolic_demangle::{Demangle, DemangleOptions};
use tokio::runtime::Handle;

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

static RQ: Lazy<Arc<Queue<StackInfo>>> = Lazy::new(|| Arc::new(ReadQueue::new(READ_TASK_COUNT)));
static PQ: Lazy<Arc<Queue<Vec<Vec<SymbolizedResult>>>>> =
    Lazy::new(|| Arc::new(ProcessQueue::new(PROCESS_TASK_COUNT)));

static LAST_UPDATED: Lazy<Arc<AtomicUsize>> = Lazy::new(|| Arc::new(AtomicUsize::new(0)));

fn bump_memlock_rlimit() -> Result<()> {
    let (ml_soft, ml_hard) = Resource::get(rlimit::Resource::MEMLOCK)?;
    if min(ml_soft, ml_hard) < 128 << 20 {
        match Resource::set(Resource::MEMLOCK, 128 << 20, 128 << 20) {
            Ok(_x) => {
                info!("raised ulimit.");
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

fn profile(args: Args, tx: SyncSender<StackInfo>, _rt: tokio::runtime::Handle) -> Result<()> {
    info!("IN PROFILE");
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
            })
            .unwrap()
        });
        0
    })?;

    thread::spawn(move || loop {});

    let rb = rbb.build()?;
    info!("CREATED RING BUFFER");

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
        let link = skel.progs_mut().profile().attach_perf_event(result)?;
        perf_fds.insert(result, link);
    }

    let mut i = 0;
    loop {
        rb.poll(Duration::from_millis(1))?;
        i += 1;
        if i >= 3000 {
            i = 0;
            let last_update = LAST_UPDATED.load(Ordering::SeqCst) as u64;
            if SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - last_update
                > 5
                && last_update != 0
            {
                break;
            };
        }
    }
    info!("DONE ONE RUN");
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
    info!("IN SYMBOLIZE");
    let sym_srcs = [SymbolSrcCfg::Process {
        pid: Some(stack_info.args.pid),
    }];
    let symbolizer = BlazeSymbolizer::new_opt(&[SymbolizerFeature::LineNumberInfo(true)]).unwrap();
    let symlist = symbolizer.symbolize(&sym_srcs, stack_info.event.ustack.as_ref());
    symlist
}

async fn process(args: Args, rt: tokio::runtime::Handle, _init: bool) -> Result<(), anyhow::Error> {
    info!("IN PROCESS");
    let (tx, rx) = sync_channel(5000);

    // let start_rc_ref = start_rc.clone();
    let rti = rt.clone();
    thread::spawn(move || {
        loop {
            match rx.recv() {
                Ok(data_chunk) => {
                    LAST_UPDATED.store(
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as usize,
                        Ordering::SeqCst,
                    );
                    let pq_ref = PQ.clone();
                    info!("READ DATA");
                    rti.clone()
                        .spawn(async move { pq_ref.push(symbolize(data_chunk)).await });
                }
                Err(_) => {
                    // cleanup between runs (needs new rx).
                    break;
                }
            }
        }
    });

    // doesn't make sense to clean up between runs.
    for _a in 1..WORKER_COUNT {
        let pq_ref = PQ.clone();
        let i_arg = args.clone();
        tokio::spawn(async move {
            loop {
                let ii_args = i_arg.clone();
                process_and_sink_data(pq_ref.pop().await, ii_args.clone())
                    .await
                    .expect("err");
                info!("SANK DATA");
            }
        });
    }
    info!("SPAWNED WORKERS");

    // trigger profile loop, wait for finish.
    profile(args, tx, rt)?;

    // done.
    Ok(())
}

async fn process_and_sink_data(
    mut symlist: Vec<Vec<SymbolizedResult>>,
    args: Args,
) -> Result<(), anyhow::Error> {
    info!("stack is");
    let mut stack_node_map: HashMap<i64, StackNode> = HashMap::new();
    let mut stack_node_data_map: HashMap<i64, StackNodeData> = HashMap::new();
    let mut profiled_binary_map: HashMap<i64, ProfiledBinary> = HashMap::new();
    let mut basename: Option<String> = None;
    if args.binary.clone().unwrap().contains('/') {
        basename = Some(
            args.clone()
                .binary
                .unwrap()
                .split('/')
                .last()
                .unwrap()
                .to_string(),
        );
    } else {
        basename = Some(args.clone().binary.unwrap());
    }

    let profiled_binary = ProfiledBinary {
        id: misc_id(args.binary.unwrap()),
        event: args.event_type.to_string(),
        build_id: None,
        basename: basename.unwrap(),
        updated_at: None,
        created_at: None,
        sample_count: 0,
        raw_data_size: 0,
        processed_data_size: 0,
    };

    let _cur_bin_id = profiled_binary.id;
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

    let mut data_out = StoData {
        stack_nodes: stack_node_map.values().map(|x| (*x).clone()).collect(),
        stack_node_datas: stack_node_data_map.values().map(|x| (*x).clone()).collect(),
        profiled_binaries: profiled_binary_map.values().map(|x| (*x).clone()).collect(),
    };

    profiled_binary_map
        .entry(profiled_binary.id)
        .and_modify(|e| e.processed_data_size += data_out.deep_size_of() as i64);

    data_out.profiled_binaries = profiled_binary_map.values().map(|x| (*x).clone()).collect();

    let client = reqwest::Client::new();
    match client.post(args.url).json(&data_out).send().await {
        Ok(x) => match x.error_for_status() {
            Ok(_x) => {}
            Err(x) => {
                error!("failed to post data: {}", x);
            }
        },
        Err(x) => {
            error!("failed to post data: {}", x);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    pretty_env_logger::init();
    let mut args = Args::parse();
    if args.pid == 0 && args.binary.is_none() {
        error!("either pid and binary or binary must be specified.");
        std::process::exit(-1);
    }
    let mut child: Option<Child> = None;
    if args.pid == 0 {
        if let Some(binary) = args.binary.clone() {
            let status = Command::new(&binary).spawn()?;
            args.pid = status.id();
            child = Some(status);
        }
    } else if args.binary.is_none() {
        error!("binary is unset, exiting");
        std::process::exit(-1);
    }

    let rt = Handle::current();

    let ctrlc = CtrlC::new().expect("cannot create Ctrl+C handler?");

    let ctrlc = tokio::spawn(ctrlc);

    let mut task = tokio::spawn(process(args.clone(), rt.clone(), true));

    loop {
        while !task.is_finished() && !ctrlc.is_finished() {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        if ctrlc.is_finished() {
            break;
        }
        if task.is_finished() {
            LAST_UPDATED.store(0_usize, Ordering::SeqCst);
            task = tokio::spawn(process(args.clone(), rt.clone(), false));
        }
    }

    if child.is_some() {
        // race condition.
        child.as_mut().unwrap().kill();
        process::exit(-1);
    }

    Ok(())
}
