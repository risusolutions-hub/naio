//! Release-mode micro-benchmarks for niao_fts hot paths.
use niao_fts::Index;
use std::collections::HashMap;
use std::time::Instant;

fn fields(body: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("body".to_string(), body.to_string());
    m
}

fn facets(cat: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("category".to_string(), cat.to_string());
    m
}

fn corpus(n: usize) -> Index {
    let mut idx = Index::new();
    let topics = [
        "rust systems programming memory safety",
        "full text search inverted index bm25 ranking",
        "vector embeddings hybrid retrieval rag pipeline",
        "database indexing btrees and hash maps",
        "distributed systems consensus raft paxos",
    ];
    for i in 0..n {
        let body = format!(
            "{} document number {i} extra tokens {}",
            topics[i % topics.len()],
            i
        );
        let cat = if i % 2 == 0 { "tech" } else { "ops" };
        idx.update(&format!("doc{i}"), fields(&body), facets(cat));
    }
    idx
}

fn bench_index(n: usize, iters: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        let _ = corpus(n);
    }
    start.elapsed().as_secs_f64() / iters as f64
}

fn bench_search(idx: &Index, iters: usize) -> f64 {
    let start = Instant::now();
    for i in 0..iters {
        let q = if i % 3 == 0 {
            "bm25 ranking"
        } else if i % 3 == 1 {
            r#""inverted index""#
        } else {
            "raft*"
        };
        let _ = idx.search(q, 10, Some("body"));
    }
    start.elapsed().as_secs_f64() / iters as f64
}

fn bench_naive_scan(docs: &[(String, String)], query: &str, iters: usize) -> f64 {
    let terms: Vec<&str> = query.split_whitespace().collect();
    let start = Instant::now();
    for _ in 0..iters {
        let mut hits = 0usize;
        for (_, body) in docs {
            let lower = body.to_lowercase();
            if terms.iter().all(|t| lower.contains(t)) {
                hits += 1;
            }
        }
        std::hint::black_box(hits);
    }
    start.elapsed().as_secs_f64() / iters as f64
}

fn main() {
    let n = 5_000;
    let idx = corpus(n);
    let index_s = bench_index(n, 3);
    let search_s = bench_search(&idx, 200);
    let docs: Vec<(String, String)> = (0..n)
        .map(|i| {
            (
                format!("doc{i}"),
                format!("full text search inverted index bm25 ranking document {i}"),
            )
        })
        .collect();
    let naive_s = bench_naive_scan(&docs, "bm25 ranking", 50);

    let docs_per_sec = n as f64 / index_s;
    let search_qps = 1.0 / search_s;
    let naive_qps = 1.0 / naive_s;

    println!("nfts_bench (release) — corpus={n} docs");
    println!(
        "index:   {:.3} ms/build  ({:.0} docs/s)",
        index_s * 1000.0,
        docs_per_sec
    );
    println!(
        "search:  {:.3} µs/query ({:.0} qps)  [BM25+phrase+prefix mix]",
        search_s * 1e6,
        search_qps
    );
    println!(
        "naive:   {:.3} µs/scan  ({:.0} qps)  [substring AND baseline]",
        naive_s * 1e6,
        naive_qps
    );
    println!("speedup vs naive: {:.1}x", naive_s / search_s);
}
