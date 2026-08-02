//! Micro-benchmark: normalization, graphemes, display width, casefold.
use niao_unicode::{
    casefold, display_width, grapheme_len, graphemes, nfc, parallel_display_width,
    parallel_normalize, NormalizationForm,
};
use std::time::Instant;

fn bench_nfc(data: &str, iters: usize) -> u64 {
    let start = Instant::now();
    let mut acc = 0usize;
    for _ in 0..iters {
        acc += nfc(data).len();
    }
    let _ = acc;
    start.elapsed().as_nanos() as u64
}

fn bench_graphemes(data: &str, iters: usize) -> (u64, usize) {
    let start = Instant::now();
    let mut n = 0usize;
    for _ in 0..iters {
        n = graphemes(data).len();
    }
    (start.elapsed().as_nanos() as u64, n)
}

fn bench_display_width(data: &str, iters: usize) -> (u64, usize) {
    let start = Instant::now();
    let mut w = 0usize;
    for _ in 0..iters {
        w = display_width(data);
    }
    (start.elapsed().as_nanos() as u64, w)
}

fn bench_casefold(data: &str, iters: usize) -> u64 {
    let start = Instant::now();
    let mut acc = 0usize;
    for _ in 0..iters {
        acc += casefold(data).len();
    }
    let _ = acc;
    start.elapsed().as_nanos() as u64
}

fn main() {
    let mixed = "Café — 日本語 naïve résumé 你好世界 🎉🇺🇸";
    let decomposed = "e\u{0301}".repeat(5000);
    let batch: Vec<String> = (0..2000).map(|i| format!("row{i}: {mixed}")).collect();

    let warmup = 3;
    let iters = 50;

    for _ in 0..warmup {
        let _ = nfc(&decomposed);
    }
    let ns = bench_nfc(&decomposed, iters);
    println!(
        "nfc 5k decomposed: {} iter, {:.0} ns/iter",
        iters,
        ns as f64 / iters as f64
    );

    for _ in 0..warmup {
        let _ = grapheme_len(mixed);
    }
    let (ns, n) = bench_graphemes(mixed, iters);
    println!(
        "graphemes mixed ({n} clusters): {} iter, {:.0} ns/iter",
        iters,
        ns as f64 / iters as f64
    );

    for _ in 0..warmup {
        let _ = display_width(mixed);
    }
    let (ns, w) = bench_display_width(mixed, iters);
    println!(
        "display_width ({w} cols): {} iter, {:.0} ns/iter",
        iters,
        ns as f64 / iters as f64
    );

    for _ in 0..warmup {
        let _ = casefold(mixed);
    }
    let ns = bench_casefold(mixed, iters);
    println!(
        "casefold mixed: {} iter, {:.0} ns/iter",
        iters,
        ns as f64 / iters as f64
    );

    for _ in 0..warmup {
        let _ = parallel_normalize(&batch, NormalizationForm::Nfc, 0);
    }
    let start = Instant::now();
    for _ in 0..10 {
        let _ = parallel_normalize(&batch, NormalizationForm::Nfc, 0);
    }
    let par_ns = start.elapsed().as_nanos() / 10;
    println!(
        "parallel_normalize 2000 strings: {:.0} ns/batch",
        par_ns as f64
    );

    for _ in 0..warmup {
        let _ = parallel_display_width(&batch, 0);
    }
    let start = Instant::now();
    for _ in 0..10 {
        let _ = parallel_display_width(&batch, 0);
    }
    let par_ns = start.elapsed().as_nanos() / 10;
    println!(
        "parallel_display_width 2000 strings: {:.0} ns/batch",
        par_ns as f64
    );
}
