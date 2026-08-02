//! Micro-benchmarks for niao_fin hot paths (release mode).
use niao_fin::{amortization, bbands, irr, macd, npv, rsi, sma};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let cf: Vec<f64> = vec![
        -15000.0, 1500.0, 2500.0, 3500.0, 4500.0, 6000.0, 7000.0, 8000.0, 9000.0, 10000.0,
    ];
    let prices: Vec<f64> = (0..10_000)
        .map(|i| 100.0 + (i as f64 * 0.01).sin() * 10.0)
        .collect();

    // Warmup
    let _ = npv(0.05, &cf).unwrap();
    let _ = irr(&[-100.0, 39.0, 59.0, 55.0, 20.0], 0.1).unwrap();
    let _ = sma(&prices, 20).unwrap();
    let _ = rsi(&prices, 14).unwrap();
    let _ = macd(&prices, 12, 26, 9).unwrap();
    let _ = bbands(&prices, 20, 2.0).unwrap();
    let _ = amortization(0.05 / 12.0, 360, 100_000.0, 0).unwrap();

    let iters = 200_000usize;
    let t0 = Instant::now();
    for _ in 0..iters {
        black_box(npv(0.05, &cf).unwrap());
    }
    let ns_npv = t0.elapsed().as_nanos() as f64 / iters as f64;
    println!(
        "npv len={} : {iters} runs, {:.1} ns/op, {:.0} ops/s",
        cf.len(),
        ns_npv.max(0.1),
        1e9 / ns_npv.max(0.1)
    );

    let iters_irr = 50_000usize;
    let irr_cf = [-100.0, 39.0, 59.0, 55.0, 20.0];
    let t1 = Instant::now();
    for _ in 0..iters_irr {
        black_box(irr(&irr_cf, 0.1).unwrap());
    }
    let ns_irr = t1.elapsed().as_nanos() as f64 / iters_irr as f64;
    println!(
        "irr len={} : {iters_irr} runs, {:.0} ns/op, {:.0} ops/s",
        irr_cf.len(),
        ns_irr,
        1e9 / ns_irr
    );

    let iters_sma = 2_000usize;
    let t2 = Instant::now();
    for _ in 0..iters_sma {
        let _ = sma(&prices, 20).unwrap();
    }
    let ns_sma = t2.elapsed().as_nanos() as f64 / iters_sma as f64;
    let mbps = (prices.len() as f64 * 8.0) / (ns_sma * 1e-9) / 1e6;
    println!(
        "sma N={} period=20: {iters_sma} runs, {:.0} ns/op, {:.1} MB/s",
        prices.len(),
        ns_sma,
        mbps
    );

    let iters_naive = 200usize;
    let t3 = Instant::now();
    for _ in 0..iters_naive {
        let period = 20usize;
        let mut out = vec![0.0; prices.len()];
        for i in (period - 1)..prices.len() {
            let mut s = 0.0;
            for j in 0..period {
                s += prices[i + 1 - period + j];
            }
            out[i] = s / period as f64;
        }
        black_box(out);
    }
    let ns_naive = t3.elapsed().as_nanos() as f64 / iters_naive as f64;
    println!(
        "sma naive O(n*p) N={}: {iters_naive} runs, {:.0} ns/op ({:.1}x slower than rolling)",
        prices.len(),
        ns_naive,
        ns_naive / ns_sma
    );

    let iters_rsi = 1_000usize;
    let t4 = Instant::now();
    for _ in 0..iters_rsi {
        let _ = rsi(&prices, 14).unwrap();
    }
    let ns_rsi = t4.elapsed().as_nanos() as f64 / iters_rsi as f64;
    println!(
        "rsi N={}: {iters_rsi} runs, {:.0} ns/op",
        prices.len(),
        ns_rsi
    );

    let iters_macd = 500usize;
    let t5 = Instant::now();
    for _ in 0..iters_macd {
        let _ = macd(&prices, 12, 26, 9).unwrap();
    }
    let ns_macd = t5.elapsed().as_nanos() as f64 / iters_macd as f64;
    println!(
        "macd N={}: {iters_macd} runs, {:.0} ns/op",
        prices.len(),
        ns_macd
    );

    let iters_amort = 10_000usize;
    let t6 = Instant::now();
    for _ in 0..iters_amort {
        let _ = amortization(0.05 / 12.0, 360, 100_000.0, 0).unwrap();
    }
    let ns_amort = t6.elapsed().as_nanos() as f64 / iters_amort as f64;
    println!(
        "amortization 360 periods: {iters_amort} runs, {:.0} ns/op",
        ns_amort
    );
}
