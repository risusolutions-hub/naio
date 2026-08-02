//! Release-mode micro-benchmarks for niao_browser.
//! Run: cargo run -p niao_browser --release --bin browser_bench

use niao_browser::{
    executable_path, js_string_literal, require_selector, require_url, LaunchConfig,
};
use std::time::Instant;

fn bench_fn<F: FnMut()>(name: &str, warmup: usize, iters: usize, mut f: F) {
    for _ in 0..warmup {
        f();
    }
    let t0 = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = t0.elapsed();
    let ns = elapsed.as_nanos() as f64 / iters as f64;
    let ops = 1e9_f64 / ns;
    println!("{name}: {ops:.0} ops/sec, {ns:.0} ns/op (n={iters})");
}

fn main() {
    println!("niao_browser release benchmarks");
    println!(
        "os={} arch={}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );

    bench_fn("require_selector", 200, 50_000, || {
        let _ = require_selector("#main .item").unwrap();
    });

    bench_fn("require_url", 200, 50_000, || {
        let _ = require_url("https://example.com/path?q=1").unwrap();
    });

    let sample = "O'Reilly \"quotes\" \\slash\nline";
    bench_fn("js_string_literal", 200, 20_000, || {
        let _ = js_string_literal(sample);
    });

    // Naive baseline: manual escape without capacity hint.
    bench_fn("js_string_literal_naive", 200, 20_000, || {
        let mut out = String::new();
        out.push('\'');
        for ch in sample.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '\'' => out.push_str("\\'"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                c => out.push(c),
            }
        }
        out.push('\'');
        std::hint::black_box(out);
    });

    bench_fn("executable_path", 5, 50, || {
        let _ = executable_path();
    });

    // Negative-path: launch with impossible executable (fast fail).
    {
        let cfg = LaunchConfig {
            executable: Some(
                if cfg!(windows) {
                    r"C:\nonexistent\chrome-nbrowser-bench.exe"
                } else {
                    "/nonexistent/chrome-nbrowser-bench"
                }
                .into(),
            ),
            timeout_ms: 200,
            ..LaunchConfig::default()
        };
        let warmup = 1usize;
        let iters = 5usize;
        for _ in 0..warmup {
            let _ = niao_browser::launch(&cfg);
        }
        let t0 = Instant::now();
        for _ in 0..iters {
            let _ = niao_browser::launch(&cfg);
        }
        let ns = t0.elapsed().as_nanos() as f64 / iters as f64;
        println!("launch_missing_exe: {:.0} ns/op (n={iters})", ns);
    }

    println!("note: live goto/screenshot/pdf throughput via cargo test -p niao_browser --release -- --ignored");
}
