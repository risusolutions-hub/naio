//! Micro-benchmarks for nyaml hot paths.
//! Run: cargo run -p niao_yaml --bin nyaml_bench --release

use niao_yaml::{emit, emit_all, parse, parse_all, EmitOptions, ParseOptions, YamlValue};
use std::time::Instant;

fn make_config_yaml(lines: usize) -> String {
    let mut s = String::from("app:\n  name: bench\n  version: 1.0\nservices:\n");
    for i in 0..lines {
        s.push_str(&format!(
            "  - id: svc{i}\n    host: host{i}.example.com\n    port: {}\n    enabled: true\n",
            8000 + (i % 100)
        ));
    }
    s.push_str("defaults: &defaults\n  timeout: 30\n  retries: 3\n");
    for i in 0..(lines / 10).max(1) {
        s.push_str(&format!("worker{i}:\n  <<: *defaults\n  queue: q{i}\n"));
    }
    s
}

fn make_multi_doc(n: usize) -> String {
    let mut parts = Vec::with_capacity(n);
    for i in 0..n {
        parts.push(format!("---\nid: {i}\nvalue: {i}\n"));
    }
    parts.join("")
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
    let config = make_config_yaml(500);
    let multi = make_multi_doc(50);
    let parsed = parse(&config, &ParseOptions::default()).expect("parse config");
    println!(
        "payload sizes: config={} multi={}",
        config.len(),
        multi.len()
    );

    let warmup = 2u32;
    let iters = 15u32;

    bench(
        "parse config ~25kB anchors",
        || {
            parse(&config, &ParseOptions::default())
                .map(|v| match v {
                    YamlValue::Mapping(m) => m.len(),
                    _ => 0,
                })
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "parse_all multi-doc x50",
        || {
            parse_all(&multi, &ParseOptions::default())
                .map(|d| d.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "emit config block",
        || {
            emit(&parsed, &EmitOptions::default())
                .map(|s| s.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "emit config flow+sort_keys",
        || {
            emit(
                &parsed,
                &EmitOptions {
                    flow: Some(true),
                    sort_keys: true,
                    ..EmitOptions::default()
                },
            )
            .map(|s| s.len())
            .unwrap_or(0)
        },
        warmup,
        iters,
    );

    let docs = parse_all(&multi, &ParseOptions::default()).expect("multi parse");
    bench(
        "emit_all x50 docs",
        || {
            emit_all(&docs, &EmitOptions::default())
                .map(|s| s.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "roundtrip parse(emit(parse))",
        || {
            let out = emit(&parsed, &EmitOptions::default()).unwrap();
            parse(&out, &ParseOptions::default())
                .map(|v| match v {
                    YamlValue::Mapping(m) => m.len(),
                    _ => 0,
                })
                .unwrap_or(0)
        },
        warmup,
        iters,
    );
}
