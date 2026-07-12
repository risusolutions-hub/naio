//! Throughput benchmark for niao_archive inflate/gzip.

use niao_archive::{deflate, gzip, gzip_decode, gzip_encode};
use std::time::Instant;

const SIZE: usize = 1024 * 1024;
const ITERS: u32 = 32;

fn payload() -> Vec<u8> {
    (0..SIZE).map(|i| ((i * 17 + i / 256) % 251) as u8).collect()
}

fn bench<F: Fn()>(name: &str, f: F) -> f64 {
    let start = Instant::now();
    for _ in 0..ITERS {
        f();
    }
    let secs = start.elapsed().as_secs_f64();
    let mb = (SIZE as f64 * ITERS as f64) / (1024.0 * 1024.0);
    let throughput = mb / secs;
    println!("{name}: {throughput:.1} MiB/s ({ITERS} x {SIZE} bytes in {secs:.3}s)");
    throughput
}

fn main() {
    let data = payload();
    let gz = gzip_encode(&data).expect("gzip encode");
    let raw_deflate = deflate::deflate(&data).expect("deflate");

    println!("=== niao_archive bench (release recommended) ===");
    let inf = bench("deflate_inflate", || {
        std::hint::black_box(deflate::inflate(&raw_deflate).unwrap());
    });
    let gz_dec = bench("gzip_decode", || {
        std::hint::black_box(gzip_decode(&gz).unwrap());
    });
    let _ = bench("gzip_encode", || {
        std::hint::black_box(gzip_encode(&data).unwrap());
    });
    let _ = bench("deflate_encode", || {
        std::hint::black_box(deflate::deflate(&data).unwrap());
    });

    println!(
        "summary: inflate={inf:.1} MiB/s gzip_decode={gz_dec:.1} MiB/s (target >= 60% of flate2/miniz on same HW)"
    );
}
