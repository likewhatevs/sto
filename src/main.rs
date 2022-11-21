use std::fs::File;
use std::io::{Cursor, Read};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

const TASK_COUNT: usize = 1000;
const WORKER_COUNT: usize = 10;

type TaskQueue = deadqueue::limited::Queue<usize>;




#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let path = "/Users/patsomaru/Documents/GitHub/sto/tests/cpu_data.txt";
    let file = File::open(path).expect("Cannot read file.");
    let queue = Arc::new(TaskQueue::new(TASK_COUNT));
    let mut buf = BufReader::new(file);

    let queue = queue.clone();
    tokio::spawn(async move {
        loop {
            let task = queue.pop().await;
            println!("worker[{}] processing task[{}] ...", worker, task);
        }
    });

    // just error out if flags are never 0.

    dbg!(read);
    Ok(())
}
