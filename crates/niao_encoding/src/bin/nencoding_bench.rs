//! Micro-benchmarks for nencoding hot paths.
//! Run: cargo run -p niao_encoding --bin nencoding_bench --release

use niao_encoding::{decode, detect, encode, transcode, DecodeErrorMode, MAX_BYTES};
use std::time::Instant;

fn make_utf8_payload(n: usize) -> Vec<u8> {
    let mut s = String::new();
    for _ in 0..n {
        s.push_str("The quick brown fox jumps over the lazy dog. 日本語 ");
    }
    encode(&s, "utf-8", false).expect("utf-8 encode")
}

fn make_shift_jis_payload(n: usize) -> Vec<u8> {
    let mut s = String::new();
    for _ in 0..n {
        s.push_str("日本語テストデータ ");
    }
    encode(&s, "shift_jis", false).expect("shift_jis encode")
}

fn make_gbk_payload(n: usize) -> Vec<u8> {
    let mut s = String::new();
    for _ in 0..n {
        s.push_str("中文测试数据 ");
    }
    encode(&s, "gbk", false).expect("gbk encode")
}

fn bench<F: Fn() -> usize>(name: &str, f: F, warmup: u32, iters: u32) {
    for _ in 0..warmup {
        let _ = f();
    }
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let t0 = Instant::now();
        let n = f();
        let elapsed = t0.elapsed().as_nanos() as u64;
        samples.push(elapsed);
        let _ = n;
    }
    samples.sort_unstable();
    let sum: u64 = samples.iter().sum();
    let mean = sum / iters as u64;
    let p50 = samples[samples.len() / 2];
    println!("{name}: mean={mean} ns p50={p50} ns (n={iters})");
}

fn main() {
    let utf8 = make_utf8_payload(2000);
    let sjis = make_shift_jis_payload(2000);
    let gbk = make_gbk_payload(2000);
    println!(
        "payload sizes: utf8={} sjis={} gbk={} (limit {MAX_BYTES})",
        utf8.len(),
        sjis.len(),
        gbk.len()
    );

    let warmup = 2u32;
    let iters = 15u32;

    bench(
        "decode utf-8 ~64kB",
        || {
            decode(&utf8, Some("utf-8"), DecodeErrorMode::Strict)
                .map(|s| s.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );
    bench(
        "decode shift_jis ~32kB",
        || {
            decode(&sjis, Some("shift_jis"), DecodeErrorMode::Strict)
                .map(|s| s.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );
    bench(
        "detect utf-8 ~64kB",
        || (detect(&utf8).confidence * 1000.0) as usize,
        warmup,
        iters,
    );
    bench(
        "detect shift_jis ~32kB",
        || (detect(&sjis).confidence * 1000.0) as usize,
        warmup,
        iters,
    );
    bench(
        "transcode sjis->utf8",
        || {
            transcode(&sjis, Some("shift_jis"), "utf-8", DecodeErrorMode::Strict)
                .map(|b| b.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );
    bench(
        "encode utf-16-le 5k lines",
        || {
            let mut text = String::new();
            for _ in 0..5000 {
                text.push_str("encode benchmark line ");
            }
            encode(&text, "utf-16-le", true)
                .map(|b| b.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );
    bench(
        "guess_decode gbk ~32kB",
        || {
            decode(&gbk, None, DecodeErrorMode::Strict)
                .map(|s| s.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );
}
