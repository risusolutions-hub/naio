//! Micro-benchmark: clean, strip, linkify, URL policy, parallel batch.
use niao_sanitize::{
    allowed_url, clean, clean_text, default_protocols, escape_html, is_html, linkify,
    parallel_clean, strip_tags, CleanOpts, LinkifyOpts,
};
use std::time::Instant;

const SAMPLE: &str = r#"
<p>Hello <b>world</b>! Visit <a href="https://example.com">example</a>.</p>
<script>alert('xss')</script>
<img src="javascript:alert(1)" onerror="alert(2)">
<div onclick="evil()">click</div>
<!-- comment -->
<p>Also see https://docs.rs/ammonia and mail support@example.com</p>
"#;

fn bench_clean(iters: usize) -> f64 {
    let opts = CleanOpts::default();
    let start = Instant::now();
    for _ in 0..iters {
        let _ = clean(SAMPLE, &opts).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_strip(iters: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        let _ = strip_tags(SAMPLE, true).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_linkify(iters: usize) -> f64 {
    let opts = LinkifyOpts::default();
    let text = "read https://example.com/path?q=1 and mail me@example.com thanks";
    let start = Instant::now();
    for _ in 0..iters {
        let _ = linkify(text, &opts).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_url_policy(iters: usize) -> f64 {
    let protocols = default_protocols();
    let urls = [
        "https://example.com",
        "javascript:alert(1)",
        "/relative",
        "mailto:a@b.c",
        "data:text/html,<script>",
    ];
    let start = Instant::now();
    let mut acc = 0u32;
    for _ in 0..iters {
        for u in &urls {
            if allowed_url(u, &protocols) {
                acc += 1;
            }
        }
    }
    let _ = acc;
    start.elapsed().as_nanos() as f64 / (iters * urls.len()) as f64
}

fn bench_escape(iters: usize) -> f64 {
    let s = "<script>alert(\"xss\")</script> & more";
    let start = Instant::now();
    for _ in 0..iters {
        let _ = escape_html(s);
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_parallel(batch: usize, iters: usize) -> f64 {
    let items: Vec<String> = (0..batch).map(|_| SAMPLE.to_string()).collect();
    let opts = CleanOpts::default();
    let threads = niao_parallel::available_threads();
    let start = Instant::now();
    for _ in 0..iters {
        let _ = parallel_clean(&items, &opts, threads).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn main() {
    let warmup = 3;
    for _ in 0..warmup {
        let _ = clean(SAMPLE, &CleanOpts::default());
    }

    let iters = 5000;
    println!(
        "fixture: {} bytes, is_html={}",
        SAMPLE.len(),
        is_html(SAMPLE)
    );
    println!("clean ({iters} iter): {:.0} ns/iter", bench_clean(iters));
    println!(
        "strip_tags ({iters} iter): {:.0} ns/iter",
        bench_strip(iters)
    );
    println!(
        "linkify ({iters} iter): {:.0} ns/iter",
        bench_linkify(iters)
    );
    println!(
        "url policy (5 urls x {iters} iter): {:.0} ns/check",
        bench_url_policy(iters)
    );
    println!(
        "escape_html ({iters} iter): {:.0} ns/iter",
        bench_escape(iters)
    );

    let batch = 200;
    let par_iters = 20;
    let threads = niao_parallel::available_threads();
    println!(
        "parallel_clean {batch} docs ({par_iters} iter, {threads} threads): {:.0} ns/iter",
        bench_parallel(batch, par_iters)
    );

    let _ = clean_text("plain & text");
}
