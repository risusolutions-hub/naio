//! Micro-benchmark: parse, CSS select, text extract, escape.
use niao_html::{
    escape, extract_text, parse_document, select_nodes, strip_tags, unescape, DocumentStore,
    TextOpts,
};
use niao_parallel::available_threads;
use scraper::Selector;
use std::time::Instant;

const PAGE: &str = r#"<!DOCTYPE html>
<html><head><title>Bench</title></head>
<body>
<nav><a href="/">Home</a><a href="/about">About</a></nav>
<main>
<article class="post"><h1>Title</h1><p>Paragraph one with <em>emphasis</em>.</p>
<p>Paragraph two.</p><ul><li>a</li><li>b</li><li>c</li></ul></article>
<article class="post"><h1>Other</h1><p>More text here.</p></article>
</main>
<footer>© 2026</footer>
</body></html>"#;

fn bench_parse(iters: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        let _ = parse_document(PAGE);
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_select(iters: usize) -> f64 {
    let doc = parse_document(PAGE);
    let sel = Selector::parse("article.post p").unwrap();
    let start = Instant::now();
    let mut n = 0usize;
    for _ in 0..iters {
        n += doc.select(&sel).count();
    }
    let _ = n;
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_select_store(iters: usize) -> f64 {
    let mut store = DocumentStore::new();
    let id = niao_html::alloc_document(&mut store, PAGE, false);
    let root = niao_html::root_node(&store, id).unwrap();
    let start = Instant::now();
    let mut n = 0usize;
    for _ in 0..iters {
        n += select_nodes(&store, root, "a").unwrap().len();
    }
    let _ = n;
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_extract_text(iters: usize) -> f64 {
    let opts = TextOpts {
        strip: true,
        separator: " ".into(),
    };
    let start = Instant::now();
    for _ in 0..iters {
        let _ = extract_text(PAGE, Some("article"), &opts).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_strip_tags(iters: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        let _ = strip_tags(PAGE);
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_escape(iters: usize) -> f64 {
    let s = "a < b & \"c\" with enough text to matter for timing";
    let start = Instant::now();
    for _ in 0..iters {
        let _ = escape(s);
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_unescape(iters: usize) -> f64 {
    let s = "a &lt; b &amp; &quot;c&quot; &#65;";
    let start = Instant::now();
    for _ in 0..iters {
        let _ = unescape(s);
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn main() {
    let threads = available_threads();
    println!("nhtml bench (threads={threads})");

    for _ in 0..3 {
        let _ = parse_document(PAGE);
    }
    println!(
        "parse_document:        {:.0} ns/iter (10k)",
        bench_parse(10_000)
    );
    println!(
        "css select article p:  {:.0} ns/iter (50k)",
        bench_select(50_000)
    );
    println!(
        "select_nodes a:        {:.0} ns/iter (50k)",
        bench_select_store(50_000)
    );
    println!(
        "extract_text:          {:.0} ns/iter (10k)",
        bench_extract_text(10_000)
    );
    println!(
        "strip_tags:            {:.0} ns/iter (50k)",
        bench_strip_tags(50_000)
    );
    println!(
        "escape:                {:.0} ns/iter (100k)",
        bench_escape(100_000)
    );
    println!(
        "unescape:              {:.0} ns/iter (100k)",
        bench_unescape(100_000)
    );
}
