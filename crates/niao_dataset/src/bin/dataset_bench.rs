//! Release-mode micro-benchmark for ndataset hot paths.

use niao_dataset::{load_csv, BatchLoader, Dataset};
use niao_frame::{DataFrame, Series};
use std::time::Instant;

fn make_dataset(n: usize) -> Dataset {
    let ids: Vec<i64> = (0..n as i64).collect();
    let labels: Vec<String> = (0..n).map(|i| format!("c{}", i % 10)).collect();
    let scores: Vec<f64> = (0..n).map(|i| (i as f64) * 0.01).collect();
    Dataset::new(
        DataFrame::new(vec![
            Series::from_i64("id", ids),
            Series::from_str("label", &labels),
            Series::from_f64("score", scores),
        ])
        .expect("frame"),
    )
}

fn bench_shuffle(n: usize) -> f64 {
    let ds = make_dataset(n);
    let t0 = Instant::now();
    let _ = ds.shuffle(42).unwrap();
    t0.elapsed().as_secs_f64() * 1000.0
}

fn bench_batch_iterate(n: usize, batch: usize) -> f64 {
    let ds = make_dataset(n);
    let t0 = Instant::now();
    let mut loader = BatchLoader::new(ds.len(), batch, true, 99, false);
    let mut batches = 0usize;
    while loader.next_indices().is_some() {
        batches += 1;
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    assert!(batches > 0);
    ms
}

fn bench_filter(n: usize) -> f64 {
    let ds = make_dataset(n);
    let t0 = Instant::now();
    let _ = ds
        .filter_eq("label", &niao_frame::FilterValue::Str("c3".into()))
        .unwrap();
    t0.elapsed().as_secs_f64() * 1000.0
}

fn bench_naive_batch(n: usize, batch: usize) -> f64 {
    let ds = make_dataset(n);
    let t0 = Instant::now();
    let mut i = 0usize;
    let mut count = 0usize;
    while i < ds.len() {
        let end = (i + batch).min(ds.len());
        let _batch: Vec<usize> = (i..end).collect();
        count += 1;
        i = end;
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    assert!(count > 0);
    ms
}

fn main() {
    let n = 100_000;
    let batch = 64;
    println!("niao_dataset bench (n={n}, batch={batch})");
    let shuffle_ms = bench_shuffle(n);
    println!(
        "  shuffle {n} rows: {shuffle_ms:.2} ms ({:.0} rows/ms)",
        n as f64 / shuffle_ms
    );

    let batch_ms = bench_batch_iterate(n, batch);
    let naive_ms = bench_naive_batch(n, batch);
    println!(
        "  batch iterate (shuffled): {batch_ms:.2} ms ({:.0} batches)",
        (n + batch - 1) / batch,
    );
    println!("  naive sequential batch (no shuffle): {naive_ms:.2} ms",);

    let filter_ms = bench_filter(n);
    println!(
        "  filter_eq on {n} rows: {filter_ms:.2} ms ({:.0} rows/ms)",
        n as f64 / filter_ms
    );

    // Optional CSV load if test fixture exists
    let fixture = "tests/fixtures/ndataset_sample.csv";
    if std::path::Path::new(fixture).exists() {
        let t0 = Instant::now();
        let ds = load_csv(fixture, true, ',').expect("csv");
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!("  load_csv {} rows: {ms:.2} ms", ds.len());
    }
}
