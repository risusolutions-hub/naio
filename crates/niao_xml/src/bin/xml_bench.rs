//! Micro-benchmark: DOM parse, emit, XPath, streaming.
use niao_xml::{
    findall, parallel_parse, parse, stream_collect, to_string_doc, StreamOpts, XmlOpts,
};
use std::time::Instant;

fn fixture(size: usize) -> String {
    let mut s = String::from(r#"<?xml version="1.0"?><catalog>"#);
    for i in 0..size {
        s.push_str(&format!(
            r#"<book id="b{i}"><title>T{i}</title><author>A{i}</author><price>{p}</price></book>"#,
            p = (i % 97) + 1
        ));
    }
    s.push_str("</catalog>");
    s
}

fn bench_parse(xml: &str, iters: usize) -> f64 {
    let opts = XmlOpts::default();
    let start = Instant::now();
    for _ in 0..iters {
        let doc = parse(xml, &opts).unwrap();
        std::hint::black_box(doc.node_count());
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_xpath(xml: &str, iters: usize) -> f64 {
    let doc = parse(xml, &XmlOpts::default()).unwrap();
    let root = doc.root.as_ref().unwrap();
    let start = Instant::now();
    for _ in 0..iters {
        let hits = findall(root, ".//book[@id='b42']").unwrap();
        std::hint::black_box(hits.len());
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_emit(xml: &str, iters: usize) -> f64 {
    let doc = parse(xml, &XmlOpts::default()).unwrap();
    let opts = XmlOpts::default();
    let start = Instant::now();
    for _ in 0..iters {
        let out = to_string_doc(&doc, &opts).unwrap();
        std::hint::black_box(out.len());
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_stream(xml: &str, iters: usize) -> f64 {
    let opts = StreamOpts::default();
    let start = Instant::now();
    for _ in 0..iters {
        let evs = stream_collect(xml, &opts).unwrap();
        std::hint::black_box(evs.len());
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_parallel(chunks: &[String], iters: usize) -> f64 {
    let opts = XmlOpts::default();
    let threads = niao_parallel::available_threads();
    let start = Instant::now();
    for _ in 0..iters {
        let out = parallel_parse(chunks, &opts, threads);
        std::hint::black_box(out.len());
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn main() {
    let small = fixture(100);
    let medium = fixture(2_000);
    let large = fixture(10_000);

    println!(
        "fixture sizes: small={}B medium={}B large={}B",
        small.len(),
        medium.len(),
        large.len()
    );

    for _ in 0..3 {
        let _ = parse(&medium, &XmlOpts::default());
    }

    println!(
        "parse medium (2k nodes, 50 iter): {:.0} ns/iter",
        bench_parse(&medium, 50)
    );
    println!(
        "xpath .//book[@id='b42'] on medium (20k iter): {:.0} ns/iter",
        bench_xpath(&medium, 20_000)
    );
    println!(
        "emit medium (30 iter): {:.0} ns/iter",
        bench_emit(&medium, 30)
    );
    println!(
        "stream medium (30 iter): {:.0} ns/iter",
        bench_stream(&medium, 30)
    );

    let chunks: Vec<String> = (0..32).map(|_| small.clone()).collect();
    println!(
        "parallel_parse 32x small ({} threads, 20 iter): {:.0} ns/iter",
        niao_parallel::available_threads(),
        bench_parallel(&chunks, 20)
    );

    println!(
        "parse large (10k nodes, 10 iter): {:.0} ns/iter",
        bench_parse(&large, 10)
    );
}
