use anyhow::{bail, Result};
use atomic_counter::{AtomicCounter, ConsistentCounter};
use blazesym::{BlazeSymbolizer, SymbolSrcCfg, SymbolizedResult, SymbolizerFeature};
use clap::Parser;
use core::time::Duration;
use std::cmp::min;
use libbpf_rs::RingBufferBuilder;
use perf_event_open_sys as perf;
use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use sto::bpftune::bpftune_bss_types::stacktrace_event;
use sto::defs::{
    Args, EventType, ProcessQueue, ReadQueue, StackInfo, PROCESS_TASK_COUNT, READ_TASK_COUNT,
    WORKER_COUNT,
};
extern crate clap;
extern crate num_cpus;
use libbpf_rs::libbpf_sys::pid_t;
use libc::{exit, getrlimit, setrlimit};
use log::{error, info};
use perf::perf_event_open;
use rlimit::Resource;
use rocket::form::validate::Len;
use sto::bpftune::*;

fn bump_memlock_rlimit() -> Result<()> {
    // let (ml_soft, ml_hard) = Resource::get(rlimit::Resource::MEMLOCK)?;
    // if min(ml_soft, ml_hard) < 128 << 20 {
    //     match Resource::set(Resource::MEMLOCK, 128<<20,128<<20){
    //         Ok(x) => {
    //             info!("raised ulimit.");
    //         },
    //         Err(x) => {
    //             bail!("unable to raise memlock limit and memlock limit uncomfortably low. \
    //                    please run the following command and retry:\n\
    //                    ulimit -l 134217728\n\
    //                    if that fails (probably will), follow these instructions: \n\
    //                    https://unix.stackexchange.com/a/359418 and retry that.\n\
    //                    *alternatively*, just re-run this with sudo");
    //         }
    //     }
    // }
    Ok(())
}

fn profile(args: Args) -> Result<()> {
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
        symbolize(StackInfo { event: event, args: srsly_still_a_thing.clone() });
        0
    })?;
    thread::spawn(move || loop {});

    let rb = rbb.build()?;

    let mut perf_fds = HashMap::new();

    for cpu in 0..num_cpus::get() {
        let mut attrs = perf::bindings::perf_event_attr::default();
        attrs.size = std::mem::size_of::<perf::bindings::perf_event_attr>() as u32;
        match args.event_type.clone() {
            EventType::Cycles => {
                attrs.type_ = perf::bindings::PERF_TYPE_HARDWARE;
                attrs.config = perf::bindings::PERF_COUNT_HW_CPU_CYCLES as u64;
            }
            EventType::Clock => {
                attrs.type_ = perf::bindings::PERF_TYPE_SOFTWARE;
                attrs.config = perf::bindings::PERF_COUNT_SW_CPU_CLOCK as u64;
            }
        }

        attrs.__bindgen_anon_1.sample_freq = args.sample_freq.clone();
        attrs.set_freq(1);
        // attrs.set_exclude_kernel(0);
        attrs.set_exclude_hv(1);
        let result = unsafe {
            perf_event_open(
                &mut attrs,
                args.pid.clone() as pid_t,
                cpu as i32,
                -1,
                perf::bindings::PERF_FLAG_FD_CLOEXEC as u64,
            )
        };
        let link = skel.progs_mut().profile().attach_perf_event(result)?;
        perf_fds.insert(result, link);
    }

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    while running.load(Ordering::SeqCst) {
        rb.poll(Duration::from_millis(1))?;
    }

    perf_fds.capacity();
    Ok(())
}

fn symbolize(stack_info: StackInfo) -> Vec<Vec<SymbolizedResult>> {
    let sym_srcs = [SymbolSrcCfg::Process {
        pid: Some(stack_info.args.pid),
    }];
    let symbolizer = BlazeSymbolizer::new_opt(&[SymbolizerFeature::LineNumberInfo(true)]).unwrap();
    let symlist = symbolizer.symbolize(&sym_srcs, stack_info.event.ustack.as_ref());
    for i in 0..stack_info.event.ustack.len() {
        let address = stack_info.event.ustack[i];
        if symlist.len() <= i || symlist[i].is_empty() {
            continue;
        }
        let sym_results = &symlist[i];
        if sym_results.len() > 1 {
            // One address may get several results (ex, inline code)
            println!("0x{:x} ({} entries)", address, sym_results.len());

            for result in sym_results {
                let SymbolizedResult {
                    symbol,
                    start_address,
                    path,
                    line_no,
                    column,
                } = result;
                println!(
                    "    {}@0x{:#x} {}:{} {}",
                    symbol, start_address, path, line_no, column
                );
                if path != ""{
                       error!("found one");
                    std::process::exit(0);
                }
            }
        } else {
            let SymbolizedResult {
                symbol,
                start_address,
                path,
                line_no,
                column,
            } = &sym_results[0];
            println!("path: {}", path.clone());
            println!(
                "0x{:#x} {}@0x{:#x} {}:{} {}",
                address, symbol, start_address, path, line_no, column
            );
            if path != ""{
                error!("found one");
                std::process::exit(0);
            }
        }
    }
    symlist
}

async fn process(args: Args) -> Result<(), anyhow::Error> {
    let rq = Arc::new(ReadQueue::new(READ_TASK_COUNT));
    let pq = Arc::new(ProcessQueue::new(PROCESS_TASK_COUNT));
    let start_rc = Arc::new(ConsistentCounter::new(0));
    let done_rc = Arc::new(ConsistentCounter::new(0));
    for _a in 1..WORKER_COUNT {
        let rq_ref = rq.clone();
        let pq_ref = pq.clone();
        let start_rc_ref = start_rc.clone();
        tokio::spawn(async move {
            loop {
                if (start_rc_ref.clone().get() as u32) >= args.total_samples {
                    break;
                }
                start_rc_ref.inc();
                let data_chunk = rq_ref.pop().await;
                pq_ref.push(symbolize(data_chunk)).await;
            }
        });
    }
    for _a in 1..WORKER_COUNT {
        let pq_ref = pq.clone();
        let done_rc_ref = done_rc.clone();
        tokio::spawn(async move {
            loop {
                sink_data(pq_ref.pop().await).await.expect("err");
                done_rc_ref.inc();
            }
        });
    }

    // trigger profile loop, wait for finish.
    profile(args)?;

    // wait for post process finish.
    while !pq.is_empty() && !rq.is_empty() && (start_rc.get() != done_rc.get()) {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    // done.
    Ok(())
}

async fn sink_data(data: Vec<Vec<SymbolizedResult>>) -> Result<(), anyhow::Error> {
    // hash and db insert.
    drop(data);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();
    let mut args = Args::parse();
    if args.pid == 0 && args.binary.is_none(){
        error!("either pid or binary must be specified.");
        std::process::exit(-1);
    }
    if args.pid == 0 {
        if let Some(binary) = args.binary.clone() {
            let status = Command::new(binary.to_string()).spawn()?;
            args.pid = status.id();
        }
    }

    process(args).await?;
    Ok(())
}
