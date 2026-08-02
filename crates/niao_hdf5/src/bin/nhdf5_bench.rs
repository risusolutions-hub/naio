//! Micro-benchmarks for nhdf5 hot paths.
//! Run: cargo run -p niao_hdf5 --bin nhdf5_bench --release

use niao_hdf5::{
    create_dataset, create_file, dataset, read_dataset, write_dataset, CreateOpts, DynData, Mode,
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
    let mean = samples.iter().sum::<u64>() / iters as u64;
    let p50 = samples[samples.len() / 2];
    println!("{name}: mean={mean} ns p50={p50} ns (n={iters})");
}

fn main() {
    let dir = std::env::temp_dir().join("niao_nhdf5_bench");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("bench.h5");
    let path_s = path.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&path);

    let rows = 2048usize;
    let cols = 2048usize;
    let n = rows * cols;
    let data: Vec<f64> = (0..n).map(|i| (i % 997) as f64 * 0.001).collect();

    let f = create_file(&path_s, Mode::Write).expect("create");
    let opts = CreateOpts {
        chunk: Some(vec![256, 256]),
        deflate: Some(4),
        shuffle: false,
        ..CreateOpts::default()
    };
    let ds = create_dataset(&f, "matrix", &[rows, cols], &opts).expect("create ds");
    write_dataset(&ds, &DynData::F64(data.clone()), None).expect("write");
    drop(ds);
    drop(f);

    let warmup = 2u32;
    let iters = 12u32;
    println!("dataset shape=[{rows}, {cols}] elements={n}");

    bench(
        "read full f64 4M elems deflate+chunk",
        || {
            let f = create_file(&path_s, Mode::Read).unwrap();
            let ds = dataset(&f, "matrix").unwrap();
            match read_dataset(&ds, None).unwrap() {
                DynData::F64(v) => v.len(),
                _ => 0,
            }
        },
        warmup,
        iters,
    );

    bench(
        "open+create 512x512 write",
        || {
            let p = dir.join("tmp_bench.h5");
            let ps = p.to_string_lossy().to_string();
            let _ = std::fs::remove_file(&p);
            let f = create_file(&ps, Mode::Write).unwrap();
            let ds = create_dataset(&f, "x", &[512, 512], &opts).unwrap();
            let d: Vec<f64> = (0..512 * 512).map(|i| i as f64).collect();
            let n = d.len();
            write_dataset(&ds, &DynData::F64(d), None).unwrap();
            n
        },
        warmup,
        iters,
    );

    let _ = std::fs::remove_dir_all(&dir);
}
