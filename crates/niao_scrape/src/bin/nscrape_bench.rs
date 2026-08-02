//! Micro-benchmarks for nscrape hot paths (release mode).
use niao_scrape::{
    canonicalize, extract, extract_links, extract_title, is_html_ct, join, parse_sitemap,
    same_host, sitemap_urls, ExtractOpts, Robots,
};
use std::time::Instant;

const ARTICLE: &str = r#"<!doctype html>
<html lang="en"><head>
<title>Bench Article Title</title>
<meta name="description" content="Excerpt for benches.">
<meta property="og:title" content="Bench Article Title">
<meta name="author" content="Bench">
</head><body>
<nav class="nav"><a href="/">Home</a><a href="/about">About</a></nav>
<article class="post-content">
<h1>Bench Article Title</h1>
<p>Cats are wonderful companions. They purr, nap, and chase lasers with great enthusiasm every single afternoon.</p>
<p>Second paragraph adds density so the readability scorer prefers the article body over navigation chrome.</p>
<p>Third paragraph continues the story with more commas, periods, and feline facts for scoring weight.</p>
</article>
<footer class="footer">Copyright</footer>
</body></html>"#;

const ROBOTS: &str = r#"
User-agent: *
Disallow: /private
Allow: /private/ok
Crawl-delay: 0.5
Sitemap: https://ex.com/sitemap.xml
"#;

const SITEMAP: &str = r#"<?xml version="1.0"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://ex.com/a</loc><priority>0.8</priority></url>
  <url><loc>https://ex.com/b</loc><lastmod>2024-01-01</lastmod></url>
  <url><loc>https://ex.com/c</loc></url>
</urlset>"#;

fn bench(name: &str, iters: usize, mut f: impl FnMut()) -> f64 {
    // warmup
    for _ in 0..iters.min(20) {
        f();
    }
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let ns = start.elapsed().as_nanos() as f64 / iters as f64;
    let ops = 1_000_000_000.0 / ns;
    println!("{name}: {ns:.1} ns/op  ({ops:.0} ops/s)  iters={iters}");
    ns
}

fn main() {
    println!("nscrape_bench (release microbenchmarks)\n");

    let extract_ns = bench("extract", 800, || {
        let _ = extract(ARTICLE, Some("https://ex.com/x"), &ExtractOpts::default()).unwrap();
    });

    // Naive baseline: whole-document text via scraper without scoring
    let naive_ns = bench("naive_body_text", 800, || {
        let doc = scraper::Html::parse_document(ARTICLE);
        let sel = scraper::Selector::parse("body").unwrap();
        let _ = doc
            .select(&sel)
            .next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_default();
    });

    bench("extract_title", 2000, || {
        let _ = extract_title(ARTICLE).unwrap();
    });

    bench("extract_links", 1500, || {
        let _ = extract_links(ARTICLE, Some("https://ex.com/")).unwrap();
    });

    bench("parse_robots", 5000, || {
        let _ = Robots::parse(ROBOTS).unwrap();
    });

    let robots = Robots::parse(ROBOTS).unwrap();
    bench("robots_allowed", 20000, || {
        let _ = robots
            .allowed("https://ex.com/private/ok", "nscrape")
            .unwrap();
    });

    bench("parse_sitemap", 5000, || {
        let _ = parse_sitemap(SITEMAP).unwrap();
    });

    bench("sitemap_urls", 5000, || {
        let _ = sitemap_urls(SITEMAP).unwrap();
    });

    bench("canonicalize", 20000, || {
        let _ = canonicalize("https://Ex.Com/path/page#frag").unwrap();
    });

    bench("join", 20000, || {
        let _ = join("https://ex.com/a/b", "../c").unwrap();
    });

    bench("same_host", 30000, || {
        let _ = same_host("https://a.com/1", "http://A.com/2").unwrap();
    });

    bench("is_html_ct", 100000, || {
        let _ = is_html_ct("text/html; charset=utf-8");
    });

    let speedup = naive_ns / extract_ns;
    println!("\nextract vs naive_body_text: extract is {speedup:.2}x the cost of naive (lower ns better for naive)");
    println!(
        "summary: extract={:.0}ops/s robots_parse={:.0}ops/s sitemap={:.0}ops/s",
        1e9 / extract_ns,
        1e9 / {
            let start = Instant::now();
            for _ in 0..5000 {
                let _ = Robots::parse(ROBOTS).unwrap();
            }
            start.elapsed().as_nanos() as f64 / 5000.0
        },
        1e9 / {
            let start = Instant::now();
            for _ in 0..5000 {
                let _ = parse_sitemap(SITEMAP).unwrap();
            }
            start.elapsed().as_nanos() as f64 / 5000.0
        }
    );
}
