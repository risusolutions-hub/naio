//! Micro-benchmark: fnmatch, compiled matchers, walk, parallel filter.
use niao_glob::{
    compile, glob_paths, match_str, parallel_filter, walk_paths, CompileOpts, GlobOpts, WalkOpts,
};
use std::path::PathBuf;
use std::time::Instant;

fn bench_match(iters: usize) -> f64 {
    let start = Instant::now();
    let mut acc = 0u64;
    for i in 0..iters {
        let name = format!("file_{}.py", i % 1000);
        if match_str(&name, "*.py", true).unwrap() {
            acc += 1;
        }
    }
    let _ = acc;
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_compile_match(paths: &[String], patterns: &[String], iters: usize) -> f64 {
    let opts = CompileOpts::default();
    let m = compile(patterns, &opts).unwrap();
    let start = Instant::now();
    let mut acc = 0usize;
    for _ in 0..iters {
        for p in paths {
            if m.matches(p) {
                acc += 1;
            }
        }
    }
    let _ = acc;
    start.elapsed().as_nanos() as f64 / (iters * paths.len()) as f64
}

fn bench_parallel_filter(paths: &[String], iters: usize, threads: usize) -> f64 {
    let start = Instant::now();
    let mut acc = 0usize;
    for _ in 0..iters {
        acc += parallel_filter(paths, "**/*.rs", true, threads)
            .unwrap()
            .len();
    }
    let _ = acc;
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let parent = root.parent().unwrap().parent().unwrap();

    let glob_opts = GlobOpts {
        root: parent.to_path_buf(),
        recursive: true,
        ..Default::default()
    };
    let paths = glob_paths("**/*.rs", &glob_opts).unwrap();
    println!("fixture: {} .rs files under crates/", paths.len());

    let sample: Vec<String> = paths.iter().take(5000).cloned().collect();
    let patterns = vec!["crates/**/*.rs".into(), "crates/**/lib.rs".into()];

    let warmup = 5;
    let iters = 200;

    for _ in 0..warmup {
        let _ = match_str("x.py", "*.py", true);
    }
    let ns = bench_match(50_000);
    println!("fnmatch *.py (50k): {:.0} ns/match", ns);

    for _ in 0..warmup {
        let _ = compile(&patterns, &CompileOpts::default());
    }
    let ns = bench_compile_match(&sample, &patterns, iters);
    println!(
        "compiled globset ({} paths x {} iter): {:.0} ns/path",
        sample.len(),
        iters,
        ns
    );

    let threads = niao_parallel::available_threads();
    for _ in 0..warmup {
        let _ = parallel_filter(&sample, "*.rs", true, threads);
    }
    let ns = bench_parallel_filter(&sample, 30, threads);
    println!(
        "parallel_filter {} paths ({} threads, 30 iter): {:.0} ns/iter",
        sample.len(),
        threads,
        ns
    );

    let walk_opts = WalkOpts {
        root: parent.to_path_buf(),
        include: vec!["**/*.rs".into()],
        exclude: vec!["**/target/**".into()],
        gitignore: true,
        ..Default::default()
    };
    let start = Instant::now();
    let mut n = 0usize;
    for _ in 0..5 {
        n = walk_paths(&walk_opts).unwrap().len();
    }
    let ns = start.elapsed().as_nanos() as f64 / 5.0;
    println!("walk filtered ({n} hits, 5 iter): {:.0} ns/iter", ns);
}
