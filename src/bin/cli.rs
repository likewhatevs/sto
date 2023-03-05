use anyhow::{bail, Result};
use atomic_counter::{AtomicCounter, ConsistentCounter};
use blazesym::{BlazeSymbolizer, SymbolSrcCfg, SymbolizedResult};
use clap::Parser;
use core::time::Duration;
use libbpf_rs::RingBufferBuilder;
use perf_event_open_sys as perf;
use std::collections::HashMap;
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
use libc::setrlimit;
use perf::perf_event_open;
use sto::bpftune::*;

fn bump_memlock_rlimit() -> Result<()> {
    let rlimit = libc::rlimit {
        rlim_cur: 128 << 20,
        rlim_max: 128 << 20,
    };
    if unsafe { setrlimit(libc::RLIMIT_MEMLOCK, &rlimit) } != 0 {
        bail!("Failed to increase rlimit");
    }
    Ok(())
}

fn profile(args: Args) -> Result<()> {
    let mut skel_builder = BpftuneSkelBuilder::default();
    bump_memlock_rlimit()?;
    let skel_ = skel_builder.open()?;
    let mut skel = skel_.load()?;
    let mut rbb = RingBufferBuilder::new();

    rbb.add(skel.maps_mut().events(), move |data: &[u8]| {
        let mut event = stacktrace_event::default();
        plain::copy_from_bytes(&mut event, data).expect("Event data buffer was too short");
        if event.pid == 0 {
            return 0;
        }
        symbolize(StackInfo { event, args });
        0
    })?;
    thread::spawn(move || loop {});

    let rb = rbb.build()?;

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
    let symbolizer = BlazeSymbolizer::new().unwrap();
    let symlist = symbolizer.symbolize(&sym_srcs, &stack_info.event.ustack.to_vec());
    for i in 0..stack_info.event.ustack.len() {
        let address = stack_info.event.ustack[i];
        if symlist.len() <= i || symlist[i].len() == 0 {
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
            }
        } else {
            let SymbolizedResult {
                symbol,
                start_address,
                path,
                line_no,
                column,
            } = &sym_results[0];
            println!(
                "0x{:#x} {}@0x{:#x} {}:{} {}",
                address, symbol, start_address, path, line_no, column
            );
        }
    }
    return symlist;
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
                if (start_rc_ref.clone().get() as u32) >= args.samples {
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
    data;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    process(args).await?;
    Ok(())
}
