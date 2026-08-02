//! Micro-benchmarks for nzip hot paths.
//! Run: cargo run -p niao_zip --bin nzip_bench --release

use niao_zip::{
    extract_all, is_zipfile_bytes, EntryWriteOptions, ExtractOptions, OpenOptions, WriteOptions,
    ZipReader, ZipWriterHandle,
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

fn make_payload(n: usize) -> Vec<u8> {
    let mut buf = Vec::with_capacity(n);
    for i in 0..n {
        buf.push((i % 251) as u8);
    }
    buf
}

fn main() {
    let dir = std::env::temp_dir().join(format!("nzip_bench_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let zip_path = dir.join("bench.zip");
    let extract_dir = dir.join("out");
    let _ = std::fs::remove_dir_all(&extract_dir);
    let _ = std::fs::remove_file(&zip_path);

    // Build a realistic archive: 200 entries, mixed sizes.
    {
        let mut zw = ZipWriterHandle::create(&zip_path, &WriteOptions::default()).unwrap();
        for i in 0..200 {
            let name = format!("files/item_{i:04}.bin");
            let data = make_payload(4096 + (i % 17) * 512);
            zw.write_bytes(&name, &data, &EntryWriteOptions::default())
                .unwrap();
        }
        zw.mkdir("empty_dir").unwrap();
        zw.finish().unwrap();
    }

    let zip_bytes = std::fs::read(&zip_path).unwrap();
    println!(
        "archive: {} bytes, {} entries, is_zipfile={}",
        zip_bytes.len(),
        201,
        is_zipfile_bytes(&zip_bytes)
    );

    let warmup = 3u32;
    let iters = 20u32;

    bench(
        "namelist (200 entries)",
        || {
            let mut zr = ZipReader::open(&zip_path, &OpenOptions::default()).unwrap();
            zr.namelist().unwrap().len()
        },
        warmup,
        iters,
    );

    bench(
        "read single entry 8KiB",
        || {
            let mut zr = ZipReader::open(&zip_path, &OpenOptions::default()).unwrap();
            zr.read("files/item_0100.bin").unwrap().len()
        },
        warmup,
        iters,
    );

    bench(
        "read 50 entries sequential",
        || {
            let mut zr = ZipReader::open(&zip_path, &OpenOptions::default()).unwrap();
            let mut total = 0usize;
            for i in 0..50 {
                let name = format!("files/item_{i:04}.bin");
                total += zr.read(&name).unwrap().len();
            }
            total
        },
        warmup,
        iters,
    );

    bench(
        "write archive 50 entries deflated",
        || {
            let p = dir.join("write_bench.zip");
            let _ = std::fs::remove_file(&p);
            let mut zw = ZipWriterHandle::create(&p, &WriteOptions::default()).unwrap();
            let mut total = 0usize;
            for i in 0..50 {
                let data = make_payload(8192);
                total += zw
                    .write_bytes(&format!("f{i}.bin"), &data, &EntryWriteOptions::default())
                    .unwrap() as usize;
            }
            zw.finish().unwrap();
            total
        },
        warmup,
        iters,
    );

    bench(
        "extract_all parallel (200 entries)",
        || {
            let out = dir.join("extract_bench");
            let _ = std::fs::remove_dir_all(&out);
            extract_all(
                &zip_path,
                &out,
                &ExtractOptions {
                    threads: Some(niao_parallel::available_threads()),
                    ..ExtractOptions::default()
                },
            )
            .unwrap()
            .len()
        },
        warmup,
        iters,
    );

    let _ = std::fs::remove_dir_all(&dir);
}
