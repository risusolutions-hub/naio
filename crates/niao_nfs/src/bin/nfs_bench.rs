//! Micro-benchmarks for `niao_nfs` hot paths.
//! Run: cargo run -p niao_nfs --bin nfs_bench --release

use niao_nfs::{
    copy_file, copy_tree, copy_tree_opts_default, disk_usage, tree_size, write_bytes_atomic,
    AtomicWriteOpts,
};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
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

fn make_tree(root: &PathBuf, files: usize, size: usize) {
    fs::create_dir_all(root.join("sub")).unwrap();
    for i in 0..files {
        let p = root.join(format!("sub/f{i}.bin"));
        let mut f = fs::File::create(&p).unwrap();
        let buf = vec![(i % 256) as u8; size];
        f.write_all(&buf).unwrap();
    }
}

fn main() {
    let src = TempDir::new().unwrap();
    let dst_parent = TempDir::new().unwrap();
    make_tree(&src.path().to_path_buf(), 64, 16 * 1024);

    bench(
        "copyfile 1MiB",
        || {
            let s = src.path().join("sub/f0.bin");
            let d = src.path().join("copy.bin");
            let _ = copy_file(&s, &d, &Default::default()).unwrap();
            1
        },
        1,
    );

    bench(
        "copytree 64x16KiB",
        || {
            let d = dst_parent.path().join("tree_bench");
            let _ = fs::remove_dir_all(&d);
            copy_tree(src.path(), &d, &copy_tree_opts_default()).unwrap();
            64
        },
        64,
    );

    bench(
        "write_atomic 64KiB x100",
        || {
            let p = src.path().join("atomic.bin");
            let data = vec![7u8; 64 * 1024];
            for _ in 0..100 {
                write_bytes_atomic(&p, &data, &AtomicWriteOpts::default()).unwrap();
            }
            100
        },
        100,
    );

    bench(
        "tree_size 64 files",
        || {
            let _ = tree_size(src.path(), 8).unwrap();
            64
        },
        64,
    );

    bench(
        "disk_usage cwd",
        || {
            for _ in 0..1000 {
                let _ = disk_usage(src.path()).unwrap();
            }
            1000
        },
        1000,
    );
}
