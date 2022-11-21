use crate::globals::{TaskQueue, HASHER_SEED, TASK_COUNT, WORKER_COUNT};
use crate::parse::process_record;
use highway::{HighwayHash, HighwayHasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time;

pub async fn read(in_file: PathBuf, binary_identifier: String) -> Result<(), anyhow::Error> {
    let file = File::open(in_file).await?;
    let mut hasher = HighwayHasher::new(HASHER_SEED);
    hasher.append(binary_identifier.clone().as_bytes());
    let root_id = hasher.finalize64() >> 1;
    let queue = Arc::new(TaskQueue::new(TASK_COUNT));
    let buf_reader = BufReader::new(file);
    let mut lines = buf_reader.lines();
    let mut buf: Vec<String> = Vec::new();
    let queue = queue.clone();

    for _a in 1..WORKER_COUNT {
        let q_ref = queue.clone();
        let binary_identifier = binary_identifier.clone();
        tokio::spawn(async move {
            loop {
                let data_chunk = q_ref.pop().await;
                process_record(data_chunk, root_id, binary_identifier.clone()).await;
            }
        });
    }

    while let Some(line) = lines.next_line().await? {
        if line.is_empty() {
            let mut done = false;
            while !done {
                match queue.try_push(buf.clone()) {
                    Err(_x) => (),
                    Ok(_x) => {
                        done = true;
                    }
                }
                // noop 10 ms sleep.
                let _ = time::sleep(Duration::from_millis(10));
            }
            buf.clear();
        } else {
            buf.push(format!("{}", line));
        }
    }

    while !queue.is_empty() {
        let _ = time::sleep(Duration::from_millis(10));
    }
    Ok(())
}
