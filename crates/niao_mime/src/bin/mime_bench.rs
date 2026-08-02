//! Micro-benchmark: magic-byte detection, extension maps, parallel sniff.
use niao_mime::{
    from_bytes, match_bytes, parallel_from_bytes, parallel_guess_types, signature_count, Detector,
    MimeRegistry, SniffOpts, BUILTIN_SIGNATURES,
};
use std::time::Instant;

fn png_header() -> Vec<u8> {
    let mut v = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    v.extend(std::iter::repeat(0u8).take(512));
    v
}

fn zip_header() -> Vec<u8> {
    let mut v = b"PK\x03\x04\x00\x00word/document.xml".to_vec();
    v.extend(std::iter::repeat(0u8).take(512));
    v
}

fn bench_from_bytes(iters: usize) -> f64 {
    let png = png_header();
    let zip = zip_header();
    let pdf = b"%PDF-1.7 sample".to_vec();
    let samples = [png.as_slice(), zip.as_slice(), pdf.as_slice()];
    let start = Instant::now();
    let mut acc = 0u32;
    for i in 0..iters {
        let data = samples[i % samples.len()];
        if from_bytes(data, &[]).is_some() {
            acc += 1;
        }
    }
    let _ = acc;
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_guess_type(iters: usize) -> f64 {
    let reg = MimeRegistry::builtin();
    let names: Vec<String> = (0..2000)
        .map(|i| {
            format!(
                "file_{}.{}",
                i % 50,
                ["png", "pdf", "html", "rs", "mp4"][i % 5]
            )
        })
        .collect();
    let start = Instant::now();
    let mut acc = 0usize;
    for _ in 0..iters {
        for n in &names {
            if reg.guess_type(n, false).mime.is_some() {
                acc += 1;
            }
        }
    }
    let _ = acc;
    start.elapsed().as_nanos() as f64 / (iters * names.len()) as f64
}

fn bench_parallel_bytes(batches: &[Vec<u8>], iters: usize, threads: usize) -> f64 {
    let start = Instant::now();
    let mut acc = 0usize;
    for _ in 0..iters {
        acc += parallel_from_bytes(batches, &[], threads)
            .into_iter()
            .filter(|m| m.is_some())
            .count();
    }
    let _ = acc;
    start.elapsed().as_nanos() as f64 / (iters * batches.len()) as f64
}

fn main() {
    println!(
        "fixture: {} builtin signatures, {} ext pairs",
        signature_count(),
        MimeRegistry::builtin().builtin_extension_count()
    );

    let warmup = 3;
    let iters = 100_000;
    for _ in 0..warmup {
        let _ = match_bytes(png_header().as_slice(), &[]);
    }
    let ns = bench_from_bytes(iters);
    println!("from_bytes mixed ({iters} iter): {ns:.0} ns/call");

    for _ in 0..warmup {
        let _ = MimeRegistry::builtin().guess_type("x.png", false);
    }
    let ns = bench_guess_type(50);
    println!("guess_type (2000 names x 50 iter): {ns:.0} ns/name");

    let batches: Vec<Vec<u8>> = (0..5000)
        .map(|i| {
            if i % 3 == 0 {
                png_header()
            } else if i % 3 == 1 {
                zip_header()
            } else {
                b"%PDF-1.4".to_vec()
            }
        })
        .collect();
    let threads = niao_parallel::available_threads();
    for _ in 0..warmup {
        let _ = parallel_from_bytes(&batches[..100], &[], threads);
    }
    let ns = bench_parallel_bytes(&batches, 20, threads);
    println!(
        "parallel_from_bytes ({} batches x 20 iter, {} threads): {ns:.0} ns/batch",
        batches.len(),
        threads
    );

    let det = Detector::default();
    let opts = SniffOpts::default();
    let start = Instant::now();
    let mut n = 0usize;
    for sig in BUILTIN_SIGNATURES.iter().take(20) {
        if det.detect_bytes(sig.bytes).is_some() {
            n += 1;
        }
    }
    let _ = opts;
    let ns = start.elapsed().as_nanos() as f64 / 20.0;
    println!("detector custom path (20 sigs): {ns:.0} ns/call (matched {n})");

    let names: Vec<String> = (0..10_000).map(|i| format!("f_{}.json", i)).collect();
    let start = Instant::now();
    let _ = parallel_guess_types(&names, &MimeRegistry::builtin(), false, threads);
    let ns = start.elapsed().as_nanos() as f64 / names.len() as f64;
    println!("parallel_guess_types (10k names, {threads} threads): {ns:.0} ns/name");
}
