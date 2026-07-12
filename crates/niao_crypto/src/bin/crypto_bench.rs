//! SHA-256 throughput on 100 MiB.

use niao_crypto::Sha256;
use std::time::Instant;

const SIZE: usize = 100 * 1024 * 1024;
const CHUNK: usize = 64 * 1024;

fn main() {
    let data: Vec<u8> = (0..SIZE).map(|i| (i % 251) as u8).collect();
    let start = Instant::now();
    let mut h = Sha256::new();
    for chunk in data.chunks(CHUNK) {
        h.update(chunk);
    }
    let _digest = h.finalize();
    let secs = start.elapsed().as_secs_f64();
    let mb = SIZE as f64 / (1024.0 * 1024.0);
    println!(
        "sha256_100mb: {:.1} MiB/s ({mb:.0} MiB in {secs:.3}s)",
        mb / secs
    );
}
