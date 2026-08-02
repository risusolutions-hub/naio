//! Micro-benchmark: line diff, unified diff, char diff, patch apply, merge, parallel batch.
use niao_parallel::available_threads;
use niao_textdiff::{
    char_diff, compare, merge, parallel_diff, patch_apply, patch_make, unified, DiffOpts, DiffPair,
    MergeOpts,
};
use std::time::Instant;

fn fixture_lines(n: usize) -> (String, String) {
    let mut a = String::new();
    let mut b = String::new();
    for i in 0..n {
        a.push_str(&format!("line {i} unchanged\n"));
        if i % 17 == 0 {
            b.push_str(&format!("line {i} CHANGED\n"));
        } else if i % 113 == 0 {
            b.push_str(&format!("line {i} inserted extra\n"));
        } else {
            b.push_str(&format!("line {i} unchanged\n"));
        }
    }
    (a, b)
}

fn bench_compare(iters: usize, n: usize) -> f64 {
    let (a, b) = fixture_lines(n);
    let opts = DiffOpts::default();
    let start = Instant::now();
    for _ in 0..iters {
        let _ = compare(&a, &b, &opts).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_unified(iters: usize, n: usize) -> f64 {
    let (a, b) = fixture_lines(n);
    let opts = DiffOpts::default();
    let start = Instant::now();
    for _ in 0..iters {
        let _ = unified(&a, &b, &opts).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_char_diff(iters: usize) -> f64 {
    let a = "The quick brown fox jumps over the lazy dog. ".repeat(200);
    let b = a.replace("brown", "BR0WN").replace("lazy", "sleepy");
    let opts = DiffOpts::default();
    let start = Instant::now();
    for _ in 0..iters {
        let _ = char_diff(&a, &b, &opts).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_patch(iters: usize, n: usize) -> f64 {
    let (a, b) = fixture_lines(n);
    let opts = DiffOpts::default();
    let patch = patch_make(&a, &b, &opts).unwrap();
    let start = Instant::now();
    for _ in 0..iters {
        let _ = patch_apply(&a, &patch, &opts).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_merge(iters: usize, n: usize) -> f64 {
    let (base, ours) = fixture_lines(n);
    let (_, theirs) = fixture_lines(n);
    let opts = MergeOpts::default();
    let start = Instant::now();
    for _ in 0..iters {
        let _ = merge(&base, &ours, &theirs, &opts).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_parallel(pairs: usize, threads: usize, iters: usize) -> f64 {
    let mut batch = Vec::with_capacity(pairs);
    for i in 0..pairs {
        let (a, b) = fixture_lines(200 + (i % 50));
        batch.push(DiffPair { from: a, to: b });
    }
    let opts = DiffOpts::default();
    let start = Instant::now();
    for _ in 0..iters {
        let _ = parallel_diff(&batch, &opts, threads).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn main() {
    let warmup = 3;
    for _ in 0..warmup {
        let _ = compare("a\n", "b\n", &DiffOpts::default());
    }

    let n = 5_000;
    let iters = 50;
    println!("fixture: {n} lines per side");
    println!(
        "compare ({iters} iter): {:.0} ns/iter",
        bench_compare(iters, n)
    );
    println!(
        "unified ({iters} iter): {:.0} ns/iter",
        bench_unified(iters, n)
    );
    println!(
        "char_diff (200x sentence, 20 iter): {:.0} ns/iter",
        bench_char_diff(20)
    );
    println!(
        "patch_apply ({iters} iter): {:.0} ns/iter",
        bench_patch(iters, n)
    );
    println!(
        "merge3 ({iters} iter): {:.0} ns/iter",
        bench_merge(iters, n)
    );

    let threads = available_threads();
    println!(
        "parallel_diff 64 pairs x 200-250 lines ({} threads, 10 iter): {:.0} ns/iter",
        threads,
        bench_parallel(64, threads, 10)
    );
}
