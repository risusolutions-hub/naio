//! Micro-benchmarks for niao_dsp hot paths (release mode).
use niao_dsp::{
    convolve, fftconvolve, firwin, hann, lfilter, resample, spectrogram, stft, ConvMode,
    SpectralOpts,
};
use std::f64::consts::PI;
use std::time::Instant;

fn main() {
    let n = 8192usize;
    let x: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 440.0 * i as f64 / 8000.0).sin())
        .collect();
    let h = firwin(63, &[0.1], "hamming", true, 2.0).unwrap();

    // Warmup
    let _ = convolve(&x, &h, ConvMode::Same).unwrap();
    let _ = fftconvolve(&x, &h, ConvMode::Same).unwrap();

    let iters = 200usize;
    let t0 = Instant::now();
    for _ in 0..iters {
        let _ = convolve(&x, &h, ConvMode::Same).unwrap();
    }
    let e0 = t0.elapsed();
    let ns = e0.as_nanos() as f64 / iters as f64;
    let mbps = (n as f64 * 8.0) / (ns * 1e-9) / 1e6;
    println!(
        "convolve same N={n} taps={}: {iters} runs, {:.0} ns/op, {:.1} MB/s signal",
        h.len(),
        ns,
        mbps
    );

    let iters_f = 500usize;
    let t1 = Instant::now();
    for _ in 0..iters_f {
        let _ = fftconvolve(&x, &h, ConvMode::Same).unwrap();
    }
    let e1 = t1.elapsed();
    let ns1 = e1.as_nanos() as f64 / iters_f as f64;
    println!(
        "fftconvolve same N={n} taps={}: {iters_f} runs, {:.0} ns/op",
        h.len(),
        ns1
    );

    let t2 = Instant::now();
    let wi = 50_000usize;
    for _ in 0..wi {
        let _ = hann(1024);
    }
    let e2 = t2.elapsed();
    println!(
        "hann(1024): {wi} runs, {:.0} ns/op",
        e2.as_nanos() as f64 / wi as f64
    );

    let t3 = Instant::now();
    let ri = 1_000usize;
    for _ in 0..ri {
        let _ = resample(&x, n / 2).unwrap();
    }
    let e3 = t3.elapsed();
    println!(
        "resample {n}->{}: {ri} runs, {:.0} ns/op",
        n / 2,
        e3.as_nanos() as f64 / ri as f64
    );

    let opts = SpectralOpts {
        fs: 8000.0,
        window: "hann".into(),
        nperseg: 256,
        noverlap: Some(128),
        nfft: Some(256),
    };
    let t4 = Instant::now();
    let si = 200usize;
    for _ in 0..si {
        let _ = stft(&x, &opts).unwrap();
    }
    let e4 = t4.elapsed();
    println!(
        "stft N={n} nperseg=256: {si} runs, {:.0} ns/op",
        e4.as_nanos() as f64 / si as f64
    );

    let t5 = Instant::now();
    for _ in 0..si {
        let _ = spectrogram(&x, &opts).unwrap();
    }
    let e5 = t5.elapsed();
    println!(
        "spectrogram N={n}: {si} runs, {:.0} ns/op",
        e5.as_nanos() as f64 / si as f64
    );

    let b = h.clone();
    let a = vec![1.0];
    let t6 = Instant::now();
    let li = 500usize;
    for _ in 0..li {
        let _ = lfilter(&b, &a, &x).unwrap();
    }
    let e6 = t6.elapsed();
    println!(
        "lfilter FIR taps={} N={n}: {li} runs, {:.0} ns/op",
        b.len(),
        e6.as_nanos() as f64 / li as f64
    );

    let long_a: Vec<f64> = (0..4096).map(|i| (i as f64 * 0.01).sin()).collect();
    let long_b: Vec<f64> = (0..4096).map(|i| (i as f64 * 0.02).cos()).collect();
    let _ = fftconvolve(&long_a[..64], &long_b[..64], ConvMode::Full).unwrap(); // warmup

    let iters_long = 50usize;
    let t_d = Instant::now();
    for _ in 0..iters_long {
        let _ = convolve(&long_a, &long_b, ConvMode::Full).unwrap();
    }
    let ns_direct_long = t_d.elapsed().as_nanos() as f64 / iters_long as f64;

    let t_f = Instant::now();
    for _ in 0..iters_long {
        let _ = fftconvolve(&long_a, &long_b, ConvMode::Full).unwrap();
    }
    let ns_fft_long = t_f.elapsed().as_nanos() as f64 / iters_long as f64;
    println!(
        "long×long convolve N=4096: direct {:.0} ns/op, fft {:.0} ns/op, speedup {:.2}x",
        ns_direct_long,
        ns_fft_long,
        ns_direct_long / ns_fft_long
    );
}
