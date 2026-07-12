//! Executor throughput benchmark.

use niao_io::spawn;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

const JOBS: u32 = 200_000;

fn main() {
    let done = Arc::new(AtomicU32::new(0));
    let start = Instant::now();
    for _ in 0..JOBS {
        let d = Arc::clone(&done);
        spawn(move || {
            d.fetch_add(1, Ordering::Relaxed);
        });
    }
    while done.load(Ordering::Relaxed) < JOBS {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let secs = start.elapsed().as_secs_f64();
    println!("io_spawn_{JOBS}: {:.0} jobs/s", JOBS as f64 / secs);
}
