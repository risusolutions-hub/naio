//! Micro-benchmark for niao_when hot paths.
use niao_when::{batch_parse, parse, search, ParseOptions};
use std::time::Instant;

fn main() {
    let opts = ParseOptions::default();
    let phrases = [
        "next friday 5pm",
        "in 2 weeks",
        "tomorrow at noon",
        "March 15, 2024",
        "2024-03-15T17:30:00Z",
        "3 days ago",
        "last monday",
        "end of month",
    ];

    let n = 200_000usize;
    let t0 = Instant::now();
    for _ in 0..n {
        let _ = parse(phrases[0], &opts).unwrap();
    }
    let e0 = t0.elapsed();
    println!(
        "parse 'next friday 5pm': {n} runs in {:.2} ms ({:.0} ns/op)",
        e0.as_secs_f64() * 1000.0,
        e0.as_nanos() as f64 / n as f64
    );

    let t1 = Instant::now();
    for _ in 0..n / 4 {
        let _ = search("please meet next friday at 5pm in the lobby", &opts).unwrap();
    }
    let e1 = t1.elapsed();
    let sn = n / 4;
    println!(
        "search embedded phrase: {sn} runs in {:.2} ms ({:.0} ns/op)",
        e1.as_secs_f64() * 1000.0,
        e1.as_nanos() as f64 / sn as f64
    );

    let corpus: Vec<String> = (0..50_000)
        .map(|i| phrases[i % phrases.len()].to_string())
        .collect();
    let t2 = Instant::now();
    let out = batch_parse(&corpus, &opts, 0);
    let e2 = t2.elapsed();
    let ok = out.iter().filter(|r| r.is_ok()).count();
    println!(
        "batch parallel: {} strings in {:.2} ms ({:.0} µs/row, {} ok)",
        corpus.len(),
        e2.as_secs_f64() * 1000.0,
        e2.as_nanos() as f64 / corpus.len() as f64 / 1000.0,
        ok
    );
}
