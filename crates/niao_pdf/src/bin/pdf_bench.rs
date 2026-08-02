//! Micro-benchmark: PDF create, extract, merge, split.

use niao_parallel::available_threads;
use niao_pdf::{
    create_builder, extract_text_bytes, finish_builder, merge_bytes, split_ranges, text,
    BuilderStore, CreateOpts, DocumentStore, ExtractOpts, TextOpts,
};
use std::time::Instant;

fn sample_pdf(pages: usize) -> Vec<u8> {
    let mut builders = BuilderStore::new();
    let b = create_builder(&mut builders, &CreateOpts::default()).unwrap();
    for i in 0..pages {
        if i > 0 {
            niao_pdf::add_page(&mut builders, b, None).unwrap();
        }
        text(
            &mut builders,
            b,
            &format!("page {i} lorem ipsum dolor sit amet"),
            &TextOpts {
                x: 72.0,
                y: 700.0,
                size: 12.0,
                ..Default::default()
            },
        )
        .unwrap();
    }
    finish_builder(&mut builders, b).unwrap()
}

fn bench_create(iters: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        let _ = sample_pdf(1);
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_extract(bytes: &[u8], iters: usize) -> f64 {
    let opts = ExtractOpts::default();
    let start = Instant::now();
    for _ in 0..iters {
        let _ = extract_text_bytes(bytes, &opts).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_merge(parts: &[Vec<u8>], iters: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        let _ = merge_bytes(parts).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_split(bytes: &[u8], iters: usize) -> f64 {
    let mut store = DocumentStore::new();
    let id = niao_pdf::open_bytes(&mut store, bytes).unwrap();
    let start = Instant::now();
    for _ in 0..iters {
        let _ = split_ranges(&store, id, &[(0, 0), (1, 1), (2, 2)]).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn main() {
    let threads = available_threads();
    println!("npdf bench (threads={threads})");

    let one = sample_pdf(1);
    let three = sample_pdf(3);
    let parts: Vec<Vec<u8>> = {
        let mut store = DocumentStore::new();
        let id = niao_pdf::open_bytes(&mut store, &three).unwrap();
        split_ranges(&store, id, &[(0, 0), (1, 1), (2, 2)]).unwrap()
    };

    for _ in 0..2 {
        let _ = extract_text_bytes(&one, &ExtractOpts::default());
    }

    println!(
        "create 1-page:         {:.0} ns/iter (500)",
        bench_create(500)
    );
    println!(
        "extract_text 1-page:   {:.0} ns/iter (200)",
        bench_extract(&one, 200)
    );
    println!(
        "merge 3 pages:         {:.0} ns/iter (200)",
        bench_merge(&parts, 200)
    );
    println!(
        "split 3-page doc:      {:.0} ns/iter (200)",
        bench_split(&three, 200)
    );
}
