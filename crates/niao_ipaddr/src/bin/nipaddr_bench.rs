//! Micro-benchmarks for nipaddr hot paths.
//! Run: cargo run -p niao_ipaddr --bin nipaddr_bench --release

use niao_ipaddr::{
    contains_many, entity_contains, parse_address, parse_ipv4_network, parse_network, IpEntity,
};
use std::time::Instant;

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
    let mean: u64 = samples.iter().sum::<u64>() / iters as u64;
    let p50 = samples[samples.len() / 2];
    println!("{name}: mean={mean} ns p50={p50} ns (n={iters})");
}

fn make_candidate_list(n: usize) -> Vec<IpEntity> {
    (0..n)
        .map(|i| {
            let o2 = (i / 256) % 256;
            let o3 = i % 256;
            let o4 = (i % 253) + 1;
            let s = format!("10.{o2}.{o3}.{o4}");
            parse_address(&s).unwrap()
        })
        .collect()
}

fn main() {
    let warmup = 3u32;
    let iters = 20u32;

    bench(
        "parse ipv4 address",
        || parse_address("192.168.1.42").map(|_| 1).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "parse ipv6 address",
        || parse_address("2001:db8::dead:beef").map(|_| 1).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "parse ipv4 /24 network",
        || {
            parse_ipv4_network("192.168.0.0/24", true)
                .map(|_| 1)
                .unwrap_or(0)
        },
        warmup,
        iters,
    );
    bench(
        "parse ipv6 /64 network",
        || parse_network("2001:db8::/64", true).map(|_| 1).unwrap_or(0),
        warmup,
        iters,
    );

    let net = parse_network("10.0.0.0/8", true).unwrap();
    let addr = parse_address("10.1.2.3").unwrap();
    bench(
        "contains /8 vs addr",
        || {
            entity_contains(&net, &addr)
                .map(|b| b as usize)
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    let candidates = make_candidate_list(10_000);
    bench(
        "contains_many 10k addrs (rayon)",
        || {
            contains_many(&net, &candidates)
                .map(|v| v.len())
                .unwrap_or(0)
        },
        warmup,
        10,
    );

    let net24 = parse_network("192.168.0.0/24", true).unwrap();
    bench(
        "contains_many 10k vs /24",
        || {
            contains_many(&net24, &candidates)
                .map(|v| v.iter().filter(|b| **b).count())
                .unwrap_or(0)
        },
        warmup,
        10,
    );
}
