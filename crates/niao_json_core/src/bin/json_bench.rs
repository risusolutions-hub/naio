//! JSON parse+serialize throughput benchmark (~5 MiB mixed document).

use niao_json_core::{parse, to_string, Value};
use std::time::Instant;

const ITERS: u32 = 32;

fn build_document(target_bytes: usize) -> String {
    let mut items = Vec::new();
    let mut i = 0i64;
    let mut size = 2;
    while size < target_bytes {
        let obj = format!(
            r#"{{"id":{i},"name":"item-{i}","active":{},"score":{},"tags":["a","b","c"]}}"#,
            i % 2 == 0,
            (i as f64) * 0.001
        );
        size += obj.len() + 1;
        items.push(obj);
        i += 1;
    }
    format!("[{}]", items.join(","))
}

fn bench(name: &str, bytes: usize, f: impl Fn()) -> f64 {
    let start = Instant::now();
    for _ in 0..ITERS {
        f();
    }
    let secs = start.elapsed().as_secs_f64();
    let mb = (bytes as f64 * ITERS as f64) / (1024.0 * 1024.0);
    let throughput = mb / secs;
    println!("{name}: {throughput:.1} MiB/s ({ITERS} iters in {secs:.3}s)");
    throughput
}

static DOC: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static PARSED: std::sync::OnceLock<Value> = std::sync::OnceLock::new();

fn doc() -> &'static str {
    DOC.get_or_init(|| build_document(5 * 1024 * 1024))
}

fn doc_bytes() -> usize {
    doc().len()
}

fn parsed() -> &'static Value {
    PARSED.get_or_init(|| parse(doc()).expect("parse doc"))
}

fn main() {
    let bytes = doc_bytes();
    let _ = parsed();
    println!("=== niao_json_core bench ({} bytes, release recommended) ===", bytes);
    let parse_tp = bench("parse", bytes, || {
        std::hint::black_box(parse(doc()).unwrap());
    });
    let ser_tp = bench("serialize", bytes, || {
        std::hint::black_box(to_string(parsed()));
    });
    let round = bench("parse+serialize", bytes, || {
        let v = parse(doc()).unwrap();
        std::hint::black_box(to_string(&v));
    });
    println!("summary: parse={parse_tp:.1} MiB/s serialize={ser_tp:.1} MiB/s round={round:.1} MiB/s");
}
