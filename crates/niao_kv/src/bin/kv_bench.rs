//! Micro-benchmarks for `niao_kv` hot paths (run with `--release`).

use niao_kv::{ScanOptions, Store, DEFAULT_TABLE};
use std::time::Instant;

fn main() {
    let n = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(50_000usize);

    println!("niao_kv bench  n={n}");

    let store = Store::memory().expect("memory db");

    // Sequential put
    let t0 = Instant::now();
    {
        let mut txn = store.begin_write().unwrap();
        for i in 0..n {
            let k = format!("k{i:08}");
            let v = format!("v{i}");
            txn.put(DEFAULT_TABLE, k.as_bytes(), v.as_bytes()).unwrap();
        }
        txn.commit().unwrap();
    }
    let put_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let put_ops = n as f64 / (put_ms / 1000.0);
    println!(
        "put_txn     {n} keys in {put_ms:.2} ms  ({put_ops:.0} ops/sec, {:.0} ns/op)",
        1e9 / put_ops
    );

    // Random get
    let t1 = Instant::now();
    let mut hits = 0usize;
    for i in (0..n).step_by(7) {
        let k = format!("k{i:08}");
        if store.get(DEFAULT_TABLE, k.as_bytes()).unwrap().is_some() {
            hits += 1;
        }
    }
    let get_n = (n + 6) / 7;
    let get_ms = t1.elapsed().as_secs_f64() * 1000.0;
    let get_ops = get_n as f64 / (get_ms / 1000.0);
    println!(
        "get_rand    {get_n} gets ({hits} hits) in {get_ms:.2} ms  ({get_ops:.0} ops/sec, {:.0} ns/op)",
        1e9 / get_ops
    );

    // Prefix scan
    let t2 = Instant::now();
    let opts = ScanOptions {
        prefix: Some(b"k0000".to_vec()),
        ..ScanOptions::default()
    };
    let pairs = store.scan(DEFAULT_TABLE, &opts).unwrap();
    let scan_ms = t2.elapsed().as_secs_f64() * 1000.0;
    let bytes: usize = pairs.iter().map(|p| p.key.len() + p.value.len()).sum();
    let mb_s = (bytes as f64 / 1e6) / (scan_ms / 1000.0).max(1e-9);
    println!(
        "prefix_scan {} pairs, {bytes} bytes in {scan_ms:.2} ms  ({mb_s:.1} MB/s)",
        pairs.len()
    );

    // Naive HashMap baseline for put
    use std::collections::HashMap;
    let t3 = Instant::now();
    let mut map = HashMap::with_capacity(n);
    for i in 0..n {
        map.insert(format!("k{i:08}"), format!("v{i}"));
    }
    let base_ms = t3.elapsed().as_secs_f64() * 1000.0;
    let base_ops = n as f64 / (base_ms / 1000.0);
    println!("baseline_hm {n} HashMap inserts in {base_ms:.2} ms  ({base_ops:.0} ops/sec)");
    println!(
        "put vs HashMap: {:.2}x slower (expected — ACID + B-tree)",
        base_ops / put_ops
    );
}
