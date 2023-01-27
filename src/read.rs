use crate::globals::{TaskQueue, HASHER_SEED, TASK_COUNT, WORKER_COUNT};
use crate::parse::process_record;
use crate::structs::MapStoData;
use highway::{HighwayHash, HighwayHasher};

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use atomic_counter::{AtomicCounter, ConsistentCounter};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time;

pub async fn read_perf(in_file: PathBuf, binary_identifier: String) -> Result<(), anyhow::Error> {
    let file = File::open(in_file).await?;
    let mut hasher = HighwayHasher::new(HASHER_SEED);
    hasher.append(binary_identifier.clone().as_bytes());
    let root_id = hasher.finalize64() >> 1;
    let queue = Arc::new(TaskQueue::new(TASK_COUNT));
    let buf_reader = BufReader::new(file);
    let mut lines = buf_reader.lines();
    let mut buf: Vec<String> = Vec::new();
    let queue = queue.clone();
    let start_rc = Arc::new(ConsistentCounter::new(0));
    let done_rc = Arc::new(ConsistentCounter::new(0));
    for _a in 1..WORKER_COUNT {
        let q_ref = queue.clone();
        let binary_identifier = binary_identifier.clone();
        let start_rc_ref = start_rc.clone();
        let done_rc_ref = done_rc.clone();
        tokio::spawn(async move {
            loop {
                let data_chunk = q_ref.pop().await;
                start_rc_ref.inc();
                process_record(data_chunk, root_id, binary_identifier.clone()).await;
                done_rc_ref.inc();
            }
        });
    }

    while let Some(line) = lines.next_line().await? {
        if line.is_empty() {
            buf.push(line.to_string());
            let mut done = false;
            while !done {
                match queue.try_push(buf.clone()) {
                    Err(_x) => {
                        time::sleep(Duration::from_millis(1)).await;
                    },
                    Ok(_x) => {
                        done = true;
                    }
                }
            }
            buf.clear();
        } else {
            buf.push(line.to_string());
        }
    }

    while !queue.is_empty() || start_rc.get() != done_rc.get() {
        time::sleep(Duration::from_millis(1)).await;
    }

    Ok(())
}

// read in to map sto data to avoid issue w/ tera.
pub async fn read_sto(in_file: PathBuf) -> Result<MapStoData, anyhow::Error> {
    use std::fs::File;
    use std::io::BufReader;
    let mut infile = File::open(in_file)?;
    let bufreader = BufReader::new(&mut infile);
    // let mut de = Deserializer::new(bufreader);
    // let data_in: StoData = Deserialize::deserialize(&mut de)?;
    let data_in: MapStoData = serde_json::from_reader(bufreader)?;
    Ok(data_in)
}
