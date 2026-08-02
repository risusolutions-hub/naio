//! Micro-benchmarks for nparquet hot paths.
//! Run: cargo run -p niao_parquet --bin nparquet_bench --release

use niao_frame::{DataFrame, Series};
use niao_parquet::{
    read_parquet_bytes, write_ipc_bytes, write_parquet_bytes, ReadOptions, WriteOptions,
};
use std::time::Instant;

fn make_frame(rows: usize) -> DataFrame {
    let ids: Vec<i64> = (0..rows as i64).collect();
    let scores: Vec<f64> = (0..rows).map(|i| (i as f64) * 0.01).collect();
    let labels: Vec<String> = (0..rows).map(|i| format!("row_{i}")).collect();
    let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    DataFrame::new(vec![
        Series::from_i64("id", ids),
        Series::from_f64("score", scores),
        Series::from_str("label", &label_refs),
        Series::from_bool("active", vec![true; rows]),
    ])
    .expect("frame")
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
    let mean: u64 = samples.iter().sum::<u64>() / iters as u64;
    let p50 = samples[samples.len() / 2];
    println!("{name}: mean={mean} ns p50={p50} ns (n={iters})");
}

fn main() {
    let rows = 100_000usize;
    let df = make_frame(rows);
    let parquet = write_parquet_bytes(&df, &WriteOptions::default()).expect("encode parquet");
    let ipc = write_ipc_bytes(&df).expect("encode ipc");
    println!(
        "payload: rows={rows} parquet={} bytes ipc={} bytes",
        parquet.len(),
        ipc.len()
    );

    let warmup = 2u32;
    let iters = 12u32;
    let opts = ReadOptions::default();

    bench(
        "write_parquet 100k rows snappy",
        || {
            write_parquet_bytes(&df, &WriteOptions::default())
                .map(|b| b.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "read_parquet 100k rows",
        || {
            read_parquet_bytes(&parquet, &opts)
                .map(|d| d.nrows())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "write_ipc 100k rows",
        || write_ipc_bytes(&df).map(|b| b.len()).unwrap_or(0),
        warmup,
        iters,
    );

    bench(
        "read_ipc 100k rows",
        || {
            niao_parquet::read_ipc_bytes(&ipc, &opts)
                .map(|d| d.nrows())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "parquet roundtrip 100k rows",
        || {
            let bytes = write_parquet_bytes(&df, &WriteOptions::default()).unwrap();
            read_parquet_bytes(&bytes, &opts)
                .map(|d| d.nrows())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );
}
