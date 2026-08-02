//! Polite scraping for Niao (`nscrape`): robots.txt, rate limits, retries,
//! sitemap crawl, and article/readability extraction.
//!
//! ~scrapy, trafilatura, newspaper

mod bot;
mod crawl;
mod error;
mod extract;
mod fetch;
mod rate;
mod robots;
mod sitemap;
mod urlutil;

pub use bot::{Bot, DEFAULT_USER_AGENT};
pub use crawl::{crawl_sitemap, Crawl, Page};
pub use error::{check_len, ScrapeError, ScrapeResult, MAX_BYTES};
pub use extract::{
    extract, extract_links, extract_meta, extract_text, extract_title, parallel_extract, Article,
    ExtractOpts, LinkInfo,
};
pub use fetch::{fetch_robots_text, get, get_once, FetchResponse};
pub use rate::{Limiter, LimiterInfo};
pub use robots::Robots;
pub use sitemap::{parse_sitemap, sitemap_urls, SitemapDoc, SitemapUrl};
pub use urlutil::{
    canonicalize, default_sitemap_url, host_of, is_html_ct, join, origin, robots_url_for, same_host,
};

#[cfg(test)]
mod bench_tests {
    use crate::extract::{extract, ExtractOpts};
    use crate::robots::Robots;
    use crate::sitemap::parse_sitemap;
    use std::time::Instant;

    const ARTICLE: &str = r#"<!doctype html><html><head><title>T</title></head><body><article class="post-content"><p>Cats are wonderful companions. They purr, nap, and chase lasers with great enthusiasm every single afternoon.</p><p>Second paragraph adds density so the readability scorer prefers the article body over navigation chrome.</p></article></body></html>"#;

    /// Release microbench (run: `cargo test -p niao_scrape bench_profile --release -- --nocapture`).
    #[test]
    fn bench_profile() {
        let iters = 500usize;
        let start = Instant::now();
        for _ in 0..iters {
            let _ = extract(ARTICLE, None, &ExtractOpts::default()).unwrap();
        }
        let extract_ns = start.elapsed().as_nanos() as f64 / iters as f64;

        let robots = Robots::parse("User-agent: *\nDisallow: /private\n").unwrap();
        let start = Instant::now();
        for _ in 0..iters * 20 {
            let _ = robots
                .allowed("https://ex.com/private/ok", "nscrape")
                .unwrap();
        }
        let allow_ns = start.elapsed().as_nanos() as f64 / (iters * 20) as f64;

        let sm = r#"<?xml version="1.0"?><urlset><url><loc>https://ex.com/a</loc></url><url><loc>https://ex.com/b</loc></url></urlset>"#;
        let start = Instant::now();
        for _ in 0..iters * 10 {
            let _ = parse_sitemap(sm).unwrap();
        }
        let sitemap_ns = start.elapsed().as_nanos() as f64 / (iters * 10) as f64;

        println!(
            "bench_profile extract={extract_ns:.1} ns/op ({:.0} ops/s)",
            1e9 / extract_ns
        );
        println!(
            "bench_profile robots_allowed={allow_ns:.1} ns/op ({:.0} ops/s)",
            1e9 / allow_ns
        );
        println!(
            "bench_profile parse_sitemap={sitemap_ns:.1} ns/op ({:.0} ops/s)",
            1e9 / sitemap_ns
        );
    }
}
