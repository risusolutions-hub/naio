//! Micro-benchmarks for ngraphql hot paths.
//! Run: cargo run -p niao_graphql --bin ngraphql_bench --release

use niao_graphql::{execute, parse_document, print_document, validate, Schema};
use serde_json::json;
use std::time::Instant;

const SDL: &str = r#"
type Query {
  hero(id: ID): Character
  heroes: [Character!]!
  hello: String!
}
type Character {
  id: ID!
  name: String!
  friends: [Character!]
}
"#;

const QUERY: &str = r#"
query GetHero($id: ID) {
  hello
  hero(id: $id) {
    name
    friends { name }
  }
  heroes {
    id
    name
  }
}
"#;

fn bench<F: Fn() -> usize>(name: &str, f: F, warmup: u32, iters: u32) {
    for _ in 0..warmup {
        let _ = f();
    }
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let t0 = Instant::now();
        let n = f();
        samples.push(t0.elapsed().as_nanos() as u64);
        let _ = n;
    }
    samples.sort_unstable();
    let mean = samples.iter().sum::<u64>() / iters as u64;
    let p50 = samples[samples.len() / 2];
    let ops = 1_000_000_000u64 / mean.max(1);
    println!("{name}: mean={mean} ns/op p50={p50} ns (~{ops} ops/sec) (n={iters})");
}

fn naive_contains_parse(src: &str) -> bool {
    // Naive baseline: string scanning without a real parser
    (src.contains('{') && src.contains('}')) && (src.contains("query") || src.starts_with('{'))
}

fn main() {
    let schema = Schema::parse(SDL).expect("schema");
    let root = json!({
        "hello": "world",
        "hero": [
            {"id": "1", "name": "Luke", "friends": [{"id": "2", "name": "Leia", "friends": []}]},
            {"id": "2", "name": "Leia", "friends": []}
        ],
        "heroes": (0..50).map(|i| json!({"id": i.to_string(), "name": format!("C{i}"), "friends": []})).collect::<Vec<_>>()
    });
    let mut vars = serde_json::Map::new();
    vars.insert("id".into(), json!("1"));
    let doc = parse_document(QUERY).unwrap();
    let printed = print_document(&doc);

    let warmup = 5u32;
    let iters = 50u32;

    println!(
        "query size: {} bytes, printed: {} bytes",
        QUERY.len(),
        printed.len()
    );
    println!("heroes list: 50 items");

    bench(
        "naive string scan (baseline)",
        || {
            if naive_contains_parse(QUERY) {
                1
            } else {
                0
            }
        },
        warmup,
        iters,
    );

    bench(
        "parse document",
        || {
            parse_document(QUERY)
                .map(|d| d.definitions.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "print document",
        || {
            let _ = print_document(&doc);
            printed.len()
        },
        warmup,
        iters,
    );

    bench(
        "parse schema",
        || {
            Schema::parse(SDL)
                .map(|s| s.type_names().len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "validate",
        || {
            validate(&doc, &schema, None)
                .map(|v| if v.ok { 1 } else { 0 })
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "execute query",
        || {
            execute(&schema, QUERY, &root, &vars, None)
                .map(|r| r.data.map(|_| 1).unwrap_or(0))
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    // Throughput for a larger query string
    let big = QUERY.repeat(20);
    let big_bytes = big.len();
    bench(
        "parse large document (20x)",
        || {
            parse_document(&big)
                .map(|d| d.definitions.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );
    let mean_parse_large = {
        let mut samples = Vec::new();
        for _ in 0..iters {
            let t0 = Instant::now();
            let _ = parse_document(&big);
            samples.push(t0.elapsed().as_nanos() as u64);
        }
        samples.iter().sum::<u64>() / iters as u64
    };
    let mb_s = (big_bytes as f64) / (mean_parse_large as f64) * 1e9 / (1024.0 * 1024.0);
    println!(
        "parse large throughput: ~{mb_s:.1} MB/s ({} bytes / {} ns mean)",
        big_bytes, mean_parse_large
    );
}
