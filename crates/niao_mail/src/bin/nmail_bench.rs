//! Micro-benchmarks for nmail hot paths (release: cargo run -p niao_mail --release --bin nmail_bench).

use niao_mail::{emit, parse, Attachment, BuildSpec, EmitOptions, ParseOptions};
use std::time::Instant;

fn main() {
    let msg = BuildSpec {
        from: Some("bench@example.com".into()),
        to: vec!["to@example.com".into()],
        subject: Some("Benchmark subject with café".into()),
        text: Some("plain text body ".repeat(50)),
        html: Some(format!("<html><body>{}</body></html>", "x".repeat(2000))),
        attachments: vec![Attachment {
            filename: Some("blob.bin".into()),
            content_type: "application/octet-stream".into(),
            disposition: "attachment".into(),
            data: vec![0u8; 64 * 1024],
        }],
        auto_date: true,
        auto_message_id: true,
        ..Default::default()
    }
    .build()
    .expect("build");

    let raw = emit(&msg, &EmitOptions::default()).expect("emit");
    let raw_bytes = raw.len();

    let warmup = 20;
    let iters = 200;
    for _ in 0..warmup {
        let _ = emit(&msg, &EmitOptions::default()).unwrap();
        let _ = parse(&raw, &ParseOptions::default()).unwrap();
    }

    let t0 = Instant::now();
    for _ in 0..iters {
        let _ = emit(&msg, &EmitOptions::default()).unwrap();
    }
    let emit_ns = t0.elapsed().as_nanos() as f64 / iters as f64;
    let emit_mbs = (raw_bytes as f64) / (emit_ns / 1e9) / (1024.0 * 1024.0);

    let t1 = Instant::now();
    for _ in 0..iters {
        let _ = parse(&raw, &ParseOptions::default()).unwrap();
    }
    let parse_ns = t1.elapsed().as_nanos() as f64 / iters as f64;
    let parse_mbs = (raw_bytes as f64) / (parse_ns / 1e9) / (1024.0 * 1024.0);

    // Naive baseline: scan for headers only (lines until blank).
    let t2 = Instant::now();
    for _ in 0..iters {
        let _ = raw
            .split("\r\n\r\n")
            .next()
            .map(|h| h.lines().count())
            .unwrap_or(0);
    }
    let naive_ns = t2.elapsed().as_nanos() as f64 / iters as f64;

    println!("nmail_bench message_bytes={raw_bytes}");
    println!(
        "emit: {:.1} ns/op  {:.2} MB/s  ({:.0} ops/sec)",
        emit_ns,
        emit_mbs,
        1e9 / emit_ns
    );
    println!(
        "parse: {:.1} ns/op  {:.2} MB/s  ({:.0} ops/sec)",
        parse_ns,
        parse_mbs,
        1e9 / parse_ns
    );
    println!(
        "naive_header_scan: {:.1} ns/op  ({:.0} ops/sec)  parse/naive={:.1}x",
        naive_ns,
        1e9 / naive_ns,
        parse_ns / naive_ns
    );
}
