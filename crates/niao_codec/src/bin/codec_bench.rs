//! Throughput benchmark for niao_codec (1 MiB payload).

use niao_codec::{base64, hex, uuid::Uuid, Base64Config};
use std::time::Instant;

const SIZE: usize = 1024 * 1024;
const ITERS: u32 = 64;

fn payload() -> Vec<u8> {
    (0..SIZE).map(|i| (i % 251) as u8).collect()
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
    let encoded = base64::encode(&data, Base64Config::STANDARD);
    let hexed = hex::encode(&data);

    println!("=== niao_codec bench (release recommended) ===");
    let b64_enc = bench("b64_encode", || {
        std::hint::black_box(base64::encode(&data, Base64Config::STANDARD));
    });
    let b64_dec = bench("b64_decode", || {
        std::hint::black_box(base64::decode(&encoded, Base64Config::STANDARD).unwrap());
    });
    let hex_enc = bench("hex_encode", || {
        std::hint::black_box(hex::encode(&data));
    });
    let hex_dec = bench("hex_decode", || {
        std::hint::black_box(hex::decode(&hexed).unwrap());
    });
    let _ = bench("uuid4_gen", || {
        for _ in 0..10_000 {
            std::hint::black_box(Uuid::new_v4().to_string());
        }
    });
    let _ = bench("uuid7_gen", || {
        for _ in 0..10_000 {
            std::hint::black_box(Uuid::new_v7().to_string());
        }
    });

    println!("summary: b64_enc={b64_enc:.1} MiB/s b64_dec={b64_dec:.1} MiB/s hex_enc={hex_enc:.1} MiB/s hex_dec={hex_dec:.1} MiB/s");
}
