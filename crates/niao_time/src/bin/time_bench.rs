//! Format/parse throughput.

use niao_time::{DateTime, Timezone};
use std::time::Instant;

const ITERS: u32 = 200_000;

fn main() {
    let tz = Timezone::utc();
    let ms = 1783080000000i64;
    let dt = DateTime::from_unix_ms(ms);
    let fmt = "%Y-%m-%d %H:%M:%S";

    let start = Instant::now();
    for _ in 0..ITERS {
        let _ = dt.format(fmt, &tz).unwrap();
    }
    let fmt_secs = start.elapsed().as_secs_f64();

    let sample = dt.format(fmt, &tz).unwrap();
    let start = Instant::now();
    for _ in 0..ITERS {
        let _ = DateTime::parse(&sample, fmt, &tz).unwrap();
    }
    let parse_secs = start.elapsed().as_secs_f64();

    println!(
        "time_format_{ITERS}: {:.0} ops/s",
        ITERS as f64 / fmt_secs
    );
    println!(
        "time_parse_{ITERS}: {:.0} ops/s",
        ITERS as f64 / parse_secs
    );
}
