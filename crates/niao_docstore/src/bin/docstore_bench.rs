//! Native micro-benchmark for niao_docstore hot paths.

use niao_docstore::{DocumentStore, UpdateCond};
use serde_json::json;
use std::time::Instant;

fn bench_insert(n: usize) -> f64 {
    let mut db = DocumentStore::memory();
    let t0 = Instant::now();
    for i in 0..n {
        db.insert(None, json!({"i": i, "k": i % 50, "name": format!("u{i}")}))
            .unwrap();
    }
    t0.elapsed().as_secs_f64() * 1000.0
}

fn bench_search_scan(n: usize, queries: usize) -> f64 {
    let mut db = DocumentStore::memory();
    for i in 0..n {
        db.insert(None, json!({"i": i, "k": i % 50})).unwrap();
    }
    let q = json!({"gt": {"i": n / 2}});
    let t0 = Instant::now();
    for _ in 0..queries {
        let _ = db.search(None, &q).unwrap();
    }
    t0.elapsed().as_secs_f64() * 1000.0
}

fn bench_search_indexed(n: usize, queries: usize) -> f64 {
    let mut db = DocumentStore::memory();
    for i in 0..n {
        db.insert(None, json!({"i": i, "k": i % 50})).unwrap();
    }
    db.create_index(None, "k").unwrap();
    let q = json!({"k": 7});
    let t0 = Instant::now();
    for _ in 0..queries {
        let _ = db.search(None, &q).unwrap();
    }
    t0.elapsed().as_secs_f64() * 1000.0
}

fn bench_update(n: usize) -> f64 {
    let mut db = DocumentStore::memory();
    for i in 0..n {
        db.insert(None, json!({"i": i, "v": 0})).unwrap();
    }
    let t0 = Instant::now();
    db.update(
        None,
        &json!({"v": 1}),
        UpdateCond::Query(&json!({"gt": {"i": n / 2}})),
    )
    .unwrap();
    t0.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let n = 10_000;
    let q = 200;
    println!("niao_docstore bench (n={n}, queries={q})");
    let insert_ms = bench_insert(n);
    println!(
        "  insert {n} docs: {insert_ms:.2} ms  ({:.0} docs/s)",
        n as f64 / (insert_ms / 1000.0)
    );
    let scan_ms = bench_search_scan(n, q);
    println!(
        "  search scan {q}x: {scan_ms:.2} ms  ({:.0} qps)",
        q as f64 / (scan_ms / 1000.0)
    );
    let idx_ms = bench_search_indexed(n, q);
    println!(
        "  search indexed {q}x: {idx_ms:.2} ms  ({:.0} qps, {:.1}x vs scan)",
        q as f64 / (idx_ms / 1000.0),
        scan_ms / idx_ms
    );
    let upd_ms = bench_update(n);
    println!("  update half of {n}: {upd_ms:.2} ms");
}
