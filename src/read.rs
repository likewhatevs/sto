use crate::globals::{TaskQueue, HASHER_SEED, TASK_COUNT, WORKER_COUNT};
use crate::parse::process_record;
use crate::structs::MapStoData;
use highway::{HighwayHash, HighwayHasher};

use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

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
            buf.push(line.to_string());
            let mut done = false;
            while !done {
                match queue.try_push(buf.clone()) {
                    Err(_x) => (),
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

    while !queue.is_empty() {}
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
