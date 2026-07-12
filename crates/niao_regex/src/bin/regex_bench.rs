//! Regex throughput on ~10 MiB haystack with `\w+@\w+\.\w+` pattern.

use niao_regex::Regex;
use std::time::Instant;

const HAY_MB: usize = 10;
const ITERS: u32 = 32;

fn main() {
    let chunk = "user@example.com and ";
    let reps = (HAY_MB * 1024 * 1024) / chunk.len();
    let hay: String = chunk.repeat(reps);
    let re = Regex::new(r"[\w.+-]+@[\w.-]+\.\w+").expect("pattern");

    let start = Instant::now();
    let mut count = 0usize;
    for _ in 0..ITERS {
        count += re.find_iter(&hay).count();
    }
    let secs = start.elapsed().as_secs_f64();
    let mb = hay.len() as f64 * ITERS as f64 / (1024.0 * 1024.0);
    println!(
        "regex_find_{HAY_MB}mb: {:.1} MiB/s ({mb:.0} MiB scanned, {count} matches, {secs:.3}s)",
        mb / secs
    );
}
