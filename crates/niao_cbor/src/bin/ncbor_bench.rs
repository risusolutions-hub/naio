//! Micro-benchmarks for ncbor hot paths.
//! Run: cargo run -p niao_cbor --bin ncbor_bench --release

use niao_cbor::{
    decode, decode_all, encode, encode_canonical, CborValue, DecodeOptions, EncodeOptions,
};
use std::time::Instant;

fn make_iot_payload(n_maps: usize) -> CborValue {
    let mut pairs = Vec::with_capacity(n_maps);
    for i in 0..n_maps {
        pairs.push((
            CborValue::String(format!("sensor_{i}")),
            CborValue::Map(vec![
                (CborValue::String("id".into()), CborValue::Int(i as i128)),
                (
                    CborValue::String("temp".into()),
                    CborValue::Float(20.0 + (i as f64) * 0.1),
                ),
                (
                    CborValue::String("ts".into()),
                    CborValue::String("2026-07-13T12:00:00Z".into()),
                ),
                (
                    CborValue::String("raw".into()),
                    CborValue::Bytes(vec![i as u8; 16]),
                ),
            ]),
        ));
    }
    CborValue::Map(pairs)
}

fn bench<F: Fn() -> usize>(name: &str, f: F, warmup: u32, iters: u32) {
    for _ in 0..warmup {
        let _ = f();
    }
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let t0 = Instant::now();
        let n = f();
        samples.push(t0.elapsed().as_nanos() as u64);
        let _ = n;
    }
    samples.sort_unstable();
    let mean = samples.iter().sum::<u64>() / iters as u64;
    let p50 = samples[samples.len() / 2];
    println!("{name}: mean={mean} ns p50={p50} ns (n={iters})");
}

fn main() {
    let payload = make_iot_payload(500);
    let bytes = encode(&payload, &EncodeOptions::default()).expect("encode");
    let canonical = encode_canonical(&payload).expect("canonical");
    println!(
        "payload: 500 sensor maps, encoded={} bytes canonical={} bytes",
        bytes.len(),
        canonical.len()
    );

    let warmup = 3u32;
    let iters = 20u32;
    let dec_opts = DecodeOptions::default();
    let enc_opts = EncodeOptions::default();

    bench(
        "encode 500-map IoT doc",
        || encode(&payload, &enc_opts).map(|b| b.len()).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "encode canonical 500-map",
        || encode_canonical(&payload).map(|b| b.len()).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "decode ~37KiB",
        || decode(&bytes, &dec_opts).map(|_| 1).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "decode canonical",
        || decode(&canonical, &dec_opts).map(|_| 1).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "roundtrip encode+decode",
        || {
            let b = encode(&payload, &enc_opts).unwrap();
            decode(&b, &dec_opts).map(|_| b.len()).unwrap_or(0)
        },
        warmup,
        iters,
    );
    let seq: Vec<u8> = (0..100)
        .flat_map(|i| encode(&CborValue::Int(i), &enc_opts).unwrap())
        .collect();
    bench(
        "decode_all 100 items",
        || decode_all(&seq, &dec_opts).map(|v| v.len()).unwrap_or(0),
        warmup,
        iters,
    );
}
