use std::thread;
use chrono::Duration;

fn main() {
    loop {
        for i in 1..10000000 {
            format!("{}", i);
        }
    }
}
