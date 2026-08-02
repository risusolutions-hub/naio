//! Micro-benchmarks for `niao_tar` hot paths.
//! Run: cargo run -p niao_tar --bin tar_bench --release

use niao_tar::{pack_tree, unpack, ReadOpts, TarReader, WriteOpts};
use std::fs;
use std::io::Write;
use std::time::Instant;
use tempfile::TempDir;

fn bench<F: FnMut() -> u64>(name: &str, mut f: F, iters: u64) {
    for _ in 0..3 {
        f();
    }
    let start = Instant::now();
    let n = f();
    let elapsed = start.elapsed();
    let mean_ns = elapsed.as_nanos() as f64 / iters as f64;
    println!(
        "{name}: n={n} mean={mean_ns:.0} ns total={} ms",
        elapsed.as_millis()
    );
}

fn make_tree(root: &std::path::Path, files: usize, size: usize) {
    fs::create_dir_all(root.join("data")).unwrap();
    for i in 0..files {
        let p = root.join(format!("data/f{i}.bin"));
        let mut f = fs::File::create(&p).unwrap();
        let buf = vec![(i % 251) as u8; size];
        f.write_all(&buf).unwrap();
    }
}

fn main() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let arc_dir = TempDir::new().unwrap();
    make_tree(src.path(), 128, 8 * 1024);

    let tar_path = arc_dir.path().join("bench.tar");
    let tgz_path = arc_dir.path().join("bench.tar.gz");
    let tzst_path = arc_dir.path().join("bench.tar.zst");

    let wopts = WriteOpts::default();
    pack_tree(src.path(), &tar_path, Some("pkg"), &wopts).unwrap();
    pack_tree(
        src.path(),
        &tgz_path,
        Some("pkg"),
        &WriteOpts {
            compression: Some(niao_tar::Compression::Gz),
            level: 6,
            ..Default::default()
        },
    )
    .unwrap();
    pack_tree(
        src.path(),
        &tzst_path,
        Some("pkg"),
        &WriteOpts {
            compression: Some(niao_tar::Compression::Zst),
            level: 3,
            ..Default::default()
        },
    )
    .unwrap();

    bench(
        "pack_tree 128x8KiB .tar",
        || {
            let p = arc_dir.path().join("pack_bench.tar");
            pack_tree(src.path(), &p, Some("pkg"), &WriteOpts::default()).unwrap();
            128
        },
        128,
    );

    bench(
        "unpack .tar 128 files",
        || {
            let d = dst.path().join("u1");
            let _ = fs::remove_dir_all(&d);
            unpack(&tar_path, &d, &Default::default()).unwrap();
            128
        },
        128,
    );

    bench(
        "unpack .tar.gz 128 files",
        || {
            let d = dst.path().join("u2");
            let _ = fs::remove_dir_all(&d);
            unpack(&tgz_path, &d, &Default::default()).unwrap();
            128
        },
        128,
    );

    bench(
        "unpack .tar.zst 128 files",
        || {
            let d = dst.path().join("u3");
            let _ = fs::remove_dir_all(&d);
            unpack(&tzst_path, &d, &Default::default()).unwrap();
            128
        },
        128,
    );

    bench(
        "read member .tar.gz",
        || {
            let r = TarReader::open_path(&tgz_path, &ReadOpts::default()).unwrap();
            let names = r.names();
            let mut total = 0usize;
            for n in names {
                if n.ends_with(".bin") {
                    total += r.read(&n, niao_tar::MAX_ENTRY_BYTES).unwrap().len();
                }
            }
            total as u64
        },
        128,
    );

    bench(
        "index .tar.gz members",
        || {
            for _ in 0..50 {
                let r = TarReader::open_path(&tgz_path, &ReadOpts::default()).unwrap();
                let _ = r.names();
            }
            50
        },
        50,
    );
}
