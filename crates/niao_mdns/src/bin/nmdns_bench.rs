//! Micro-benchmarks for nmdns hot paths.
//! Run: cargo run -p niao_mdns --bin nmdns_bench --release

use niao_mdns::{
    build_query, decode_message, encode_message, pack_txt, unpack_txt, RecordType, ServiceInfo,
};
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Instant;

fn bench<F: FnMut() -> usize>(name: &str, mut f: F, warmup: u32, iters: u32) {
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
    let mean: u64 = samples.iter().sum::<u64>() / iters as u64;
    let p50 = samples[samples.len() / 2];
    let ops = 1_000_000_000u64 / mean.max(1);
    println!("{name}: mean={mean} ns/op p50={p50} ns/op ≈ {ops} ops/sec (n={iters})");
}

fn sample_service() -> ServiceInfo {
    let mut props = BTreeMap::new();
    props.insert("path".into(), "/api".into());
    props.insert("version".into(), "1.0".into());
    ServiceInfo::new(
        "BenchService",
        "_http._tcp",
        8080,
        Some("bench.local.".into()),
        vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 42))],
        props,
        0,
        0,
        120,
    )
    .unwrap()
}

fn main() {
    let warmup = 5u32;
    let iters = 50u32;

    bench(
        "build_query PTR",
        || {
            build_query("_http._tcp.local.", RecordType::Ptr)
                .map(|v| v.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    let svc = sample_service();
    let msg = svc.to_response_message(false).unwrap();
    let wire = encode_message(&msg).unwrap();
    let wire_len = wire.len();

    bench(
        "encode_service_response",
        || {
            let m = svc.to_response_message(false).unwrap();
            encode_message(&m).map(|v| v.len()).unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "decode_service_response",
        || {
            decode_message(&wire)
                .map(|m| m.answers.len() + m.additionals.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    // Throughput for decode
    {
        let rounds = 20_000u64;
        let t0 = Instant::now();
        for _ in 0..rounds {
            let _ = decode_message(&wire).unwrap();
        }
        let elapsed = t0.elapsed().as_secs_f64();
        let bytes = rounds as f64 * wire_len as f64;
        let mbs = (bytes / elapsed) / (1024.0 * 1024.0);
        println!("decode_throughput: {mbs:.1} MB/s (packet={wire_len} B, rounds={rounds})");
    }

    let mut props = BTreeMap::new();
    for i in 0..16 {
        props.insert(format!("k{i}"), format!("value-{i}"));
    }
    let packed = pack_txt(&props).unwrap();
    bench(
        "pack_txt 16 keys",
        || pack_txt(&props).map(|v| v.len()).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "unpack_txt 16 keys",
        || unpack_txt(&packed).map(|m| m.len()).unwrap_or(0),
        warmup,
        iters,
    );

    // Naive baseline: format TXT by hand with String push (no length prefixes reuse).
    bench(
        "naive_txt_baseline 16 keys",
        || {
            let mut s = String::new();
            for (k, v) in &props {
                s.push_str(k);
                s.push('=');
                s.push_str(v);
                s.push('\n');
            }
            s.len()
        },
        warmup,
        iters,
    );
}
