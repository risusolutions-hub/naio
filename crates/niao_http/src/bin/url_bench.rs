//! URL parse/join/encode throughput benchmark (alloc-light target).

use niao_http::{form_urlencode, join, parse_url, percent_decode, percent_encode, Url};
use std::time::Instant;

const PARSE_ITERS: u32 = 500_000;
const JOIN_ITERS: u32 = 250_000;
const ENCODE_ITERS: u32 = 200_000;

static FIXTURES: &[&str] = &[
    "http://example.com/",
    "https://user:pass@host.example.com:8443/path/to/resource?q=1&b=2#frag",
    "http://[::1]:8080/api/v1/items",
    "https://cdn.example.org/assets/app.js?cache=bust",
    "http://localhost:3000/search?term=hello+world",
    "https://api.service.io/v2/users/42/profile",
    "http://192.168.0.1:80/status",
    "https://example.com/a/b/c/d/e/f/g",
];

fn bench<F: Fn()>(name: &str, iters: u32, f: F) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let secs = start.elapsed().as_secs_f64();
    let ops = iters as f64 / secs;
    println!("{name}: {ops:.0} ops/s ({iters} iters in {secs:.3}s)");
    ops
}

fn main() {
    let base = parse_url("https://example.com/a/b/c?q=old").unwrap();
    let encode_sample = b"path segment with spaces & symbols=!@#";
    let encoded = percent_encode(encode_sample);

    println!("=== niao_http url bench (release recommended) ===");
    let parse_ops = bench("parse", PARSE_ITERS, || {
        for raw in FIXTURES {
            std::hint::black_box(parse_url(raw).unwrap());
        }
    });
    let join_ops = bench("join", JOIN_ITERS, || {
        std::hint::black_box(join(&base, "../d/e?x=1").unwrap());
    });
    let enc_ops = bench("percent_encode", ENCODE_ITERS, || {
        std::hint::black_box(percent_encode(encode_sample));
    });
    let dec_ops = bench("percent_decode", ENCODE_ITERS, || {
        std::hint::black_box(percent_decode(&encoded).unwrap());
    });
    let form_ops = bench("form_urlencode", ENCODE_ITERS, || {
        std::hint::black_box(form_urlencode(encode_sample));
    });
    let _ = bench("serialize", ENCODE_ITERS, || {
        std::hint::black_box(Url::parse(FIXTURES[1]).unwrap().to_string_full());
    });

    println!(
        "summary: parse={parse_ops:.0}/s join={join_ops:.0}/s enc={enc_ops:.0}/s dec={dec_ops:.0}/s form={form_ops:.0}/s"
    );
    println!("target: alloc-light — single pre-sized buffer per op, no hot-loop heap growth");
}
