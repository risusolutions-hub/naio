//! HTTP types throughput benchmark (HeaderMap, Method, StatusCode, Uri).

use niao_http::types::{HeaderMap, HeaderName, Method, StatusCode, Uri};
use std::str::FromStr;
use std::time::Instant;

const OPS: u32 = 500_000;

fn bench<F: Fn()>(name: &str, f: F) -> f64 {
    let start = Instant::now();
    for _ in 0..OPS {
        f();
    }
    let secs = start.elapsed().as_secs_f64();
    let ops_per_sec = OPS as f64 / secs;
    println!("{name}: {ops_per_sec:.0} ops/s ({OPS} ops in {secs:.3}s)");
    ops_per_sec
}

fn main() {
    let names = ["Content-Type", "Accept", "Cache-Control", "X-Request-Id"];
    let values = ["text/html", "application/json", "no-cache", "deadbeef"];

    println!("=== niao_http types bench (release recommended) ===");

    let header_insert = bench("header_insert", || {
        let mut map = HeaderMap::with_capacity(4);
        for (n, v) in names.iter().zip(values.iter()) {
            map.insert(*n, *v);
        }
        std::hint::black_box(&map);
    });

    let header_get_ci = bench("header_get_case_insensitive", || {
        let mut map = HeaderMap::with_capacity(4);
        for (n, v) in names.iter().zip(values.iter()) {
            map.insert(*n, *v);
        }
        for n in names {
            std::hint::black_box(map.get(&n.to_ascii_lowercase()));
            std::hint::black_box(map.get(&n.to_ascii_uppercase()));
        }
    });

    let header_name = bench("header_name_parse", || {
        for n in names {
            std::hint::black_box(HeaderName::from_str(n).unwrap());
        }
    });

    let method_parse = bench("method_parse", || {
        for m in ["GET", "POST", "PUT", "DELETE"] {
            std::hint::black_box(Method::from_str(m).unwrap());
        }
    });

    let status_code = bench("status_code_reason", || {
        for code in [200u16, 404, 500] {
            std::hint::black_box(StatusCode::from(code).canonical_reason());
        }
    });

    let uri_parse = bench("uri_parse", || {
        for u in [
            "/hello/world?q=1",
            "https://example.com/path",
            "http://127.0.0.1:8080/api",
        ] {
            std::hint::black_box(u.parse::<Uri>().unwrap());
        }
    });

    println!(
        "summary: header_insert={header_insert:.0}/s header_get_ci={header_get_ci:.0}/s \
         header_name={header_name:.0}/s method={method_parse:.0}/s status={status_code:.0}/s uri={uri_parse:.0}/s"
    );
}
