//! Release-mode micro-benchmarks for nblob hot paths.
//!
//! Run: `cargo run -p niao_nblob --bin nblob_bench --release`

use niao_nblob::{fs_local, fs_memory, parse, OpenMode, Vfs};
use std::time::{Duration, Instant};

fn bench(name: &str, iters: u32, mut f: impl FnMut()) {
    // warmup
    for _ in 0..3 {
        f();
    }
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = start.elapsed();
    let per = elapsed / iters;
    let ops = iters as f64 / elapsed.as_secs_f64();
    println!(
        "{name:<36} {iters:>8} iters  {:>10.3?} /op  {:>12.1} ops/s",
        per, ops
    );
}

fn bench_bytes(name: &str, total_bytes: u64, elapsed: Duration) {
    let mibs = total_bytes as f64 / (1024.0 * 1024.0);
    let mbps = mibs / elapsed.as_secs_f64();
    println!(
        "{name:<36} {:>10.2} MiB in {:>8.3?}  {:>10.1} MiB/s",
        mibs, elapsed, mbps
    );
}

fn main() {
    println!("nblob_bench (release) — {}", std::env::consts::OS);

    // URI parse
    bench("uri.parse s3://bucket/key", 200_000, || {
        let _ = parse("s3://my-bucket/path/to/object.bin").unwrap();
    });
    bench("uri.parse local path", 200_000, || {
        let _ = parse("/var/data/file.txt").unwrap();
    });

    // Memory store throughput
    let mem = fs_memory(Some("bench_root"));
    let payload = vec![0u8; 64 * 1024];
    bench("memory.write 64KiB", 5_000, || {
        mem.store.write("hot/blob.bin", &payload, None).unwrap();
    });
    bench("memory.read 64KiB", 5_000, || {
        let _ = mem.store.read("hot/blob.bin").unwrap();
    });
    bench("memory.list prefix", 5_000, || {
        let _ = mem.store.list("hot", false).unwrap();
    });

    // Local store throughput
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("nblob_bench_{stamp}"));
    let _ = std::fs::create_dir_all(&root);
    let local = fs_local(Some(root.to_str().unwrap()));
    let big = vec![0xABu8; 1024 * 1024]; // 1 MiB
    let start = Instant::now();
    let rounds = 20u64;
    for i in 0..rounds {
        local.store.write(&format!("f{i}.bin"), &big, None).unwrap();
    }
    bench_bytes(
        "local.write 1MiB x20",
        rounds * big.len() as u64,
        start.elapsed(),
    );

    let start = Instant::now();
    for i in 0..rounds {
        let _ = local.store.read(&format!("f{i}.bin")).unwrap();
    }
    bench_bytes(
        "local.read 1MiB x20",
        rounds * big.len() as u64,
        start.elapsed(),
    );

    // Naive baseline: std::fs read/write same files
    let start = Instant::now();
    for i in 0..rounds {
        let p = root.join(format!("naive_{i}.bin"));
        std::fs::write(&p, &big).unwrap();
    }
    bench_bytes(
        "baseline std::fs::write 1MiB x20",
        rounds * big.len() as u64,
        start.elapsed(),
    );

    let start = Instant::now();
    for i in 0..rounds {
        let p = root.join(format!("naive_{i}.bin"));
        let _ = std::fs::read(&p).unwrap();
    }
    bench_bytes(
        "baseline std::fs::read 1MiB x20",
        rounds * big.len() as u64,
        start.elapsed(),
    );

    // open/flush path via Vfs
    let vfs = Vfs::default();
    let uri = format!("memory://bench_root/open_path.bin");
    bench("vfs.open+write+flush memory", 2_000, || {
        let mut f = vfs.open_uri(&uri, OpenMode::Write).unwrap();
        f.write(b"hello-nblob").unwrap();
        f.flush().unwrap();
    });

    let _ = std::fs::remove_dir_all(&root);
    println!("done.");
}
