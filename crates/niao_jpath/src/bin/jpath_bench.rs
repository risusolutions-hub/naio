//! Micro-benchmark: JSON Pointer, JSONPath, JMESPath, patch, parallel batch.
use niao_jpath::{
    compile_jmes, compile_path, diff, jmes, merge_patch, parallel_find, parallel_jmes, patch_apply,
    path_find, path_search, pointer_get, pointer_set, ParallelOpts,
};
use serde_json::{json, Value};
use std::time::Instant;

fn bench_pointer_get(doc: &Value, iters: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        let _ = pointer_get(doc, "/store/book/0/author").unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_pointer_set(doc: &Value, iters: usize) -> f64 {
    let start = Instant::now();
    for i in 0..iters {
        let _ = pointer_set(doc, "/store/book/0/price", json!(i as f64)).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_jsonpath_find(doc: &Value, query: &str, iters: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        let _ = path_find(doc, query).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_jsonpath_compiled(doc: &Value, query: &str, iters: usize) -> f64 {
    let compiled = compile_path(query).unwrap();
    let start = Instant::now();
    for _ in 0..iters {
        let _ = path_search(&compiled, doc).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_jmes(doc: &Value, expr: &str, iters: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        let _ = jmes(doc, expr).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_jmes_compiled(doc: &Value, expr: &str, iters: usize) -> f64 {
    let compiled = compile_jmes(expr).unwrap();
    let start = Instant::now();
    for _ in 0..iters {
        let _ = niao_jpath::search_with_compiled(&compiled, doc).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_patch(doc: &Value, iters: usize) -> f64 {
    let patch = json!([
        {"op": "replace", "path": "/store/book/0/price", "value": 9.99}
    ]);
    let start = Instant::now();
    for _ in 0..iters {
        let _ = patch_apply(doc, &patch).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_diff(a: &Value, b: &Value, iters: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        let _ = diff(a, b);
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_merge(doc: &Value, iters: usize) -> f64 {
    let patch = json!({"store": {"book": [{"price": 1.0}]}});
    let start = Instant::now();
    for _ in 0..iters {
        let _ = merge_patch(doc, &patch).unwrap();
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn store_doc() -> Value {
    json!({
        "store": {
            "book": (0..64).map(|i| json!({
                "category": if i % 2 == 0 { "reference" } else { "fiction" },
                "author": format!("Author {i}"),
                "title": format!("Title {i}"),
                "price": 8.95 + (i as f64) * 0.1,
                "isbn": if i % 3 == 0 { json!(format!("ISBN-{i}")) } else { Value::Null },
            })).collect::<Vec<_>>(),
            "bicycle": {"color": "red", "price": 19.95}
        },
        "expensive": 10
    })
}

fn main() {
    let doc = store_doc();
    let books = 64usize;
    println!("fixture: store with {books} books");

    let warmup = 5;
    let iters = 50_000;

    for _ in 0..warmup {
        let _ = pointer_get(&doc, "/store/book/0/author");
    }
    println!(
        "pointer_get /store/book/0/author ({iters} iter): {:.0} ns/op",
        bench_pointer_get(&doc, iters)
    );

    for _ in 0..warmup {
        let _ = pointer_set(&doc, "/store/book/0/price", json!(9.99));
    }
    println!(
        "pointer_set immutable ({iters} iter): {:.0} ns/op",
        bench_pointer_set(&doc, iters)
    );

    let query = "$.store.book[*].author";
    for _ in 0..warmup {
        let _ = path_find(&doc, query);
    }
    println!(
        "jsonpath find {query} ({iters} iter): {:.0} ns/op",
        bench_jsonpath_find(&doc, query, iters)
    );

    for _ in 0..warmup {
        let c = compile_path(query).unwrap();
        let _ = path_search(&c, &doc);
    }
    println!(
        "jsonpath compiled {query} ({iters} iter): {:.0} ns/op",
        bench_jsonpath_compiled(&doc, query, iters)
    );

    let filter = "$.store.book[?(@.price < 10)]";
    let filter_iters = 10_000;
    println!(
        "jsonpath filter {filter} ({filter_iters} iter): {:.0} ns/op",
        bench_jsonpath_find(&doc, filter, filter_iters)
    );

    let jmes_expr = "store.book[*].author";
    for _ in 0..warmup {
        let _ = jmes(&doc, jmes_expr);
    }
    println!(
        "jmespath {jmes_expr} ({iters} iter): {:.0} ns/op",
        bench_jmes(&doc, jmes_expr, iters)
    );

    let jmes_pipe = "store.book[*].price | max(@)";
    for _ in 0..warmup {
        let c = compile_jmes(jmes_pipe).unwrap();
        let _ = niao_jpath::search_with_compiled(&c, &doc);
    }
    println!(
        "jmespath compiled {jmes_pipe} ({iters} iter): {:.0} ns/op",
        bench_jmes_compiled(&doc, jmes_pipe, iters)
    );

    let patch_iters = 5_000;
    println!(
        "patch_apply 2 ops ({patch_iters} iter): {:.0} ns/op",
        bench_patch(&doc, patch_iters)
    );

    let mut doc2 = doc.clone();
    doc2["store"]["book"][0]["price"] = json!(99.0);
    let diff_iters = 2_000;
    println!(
        "diff ({diff_iters} iter): {:.0} ns/op",
        bench_diff(&doc, &doc2, diff_iters)
    );

    println!(
        "merge_patch ({patch_iters} iter): {:.0} ns/op",
        bench_merge(&doc, patch_iters)
    );

    let docs: Vec<Value> = (0..256)
        .map(|i| json!({"id": i, "payload": doc.clone()}))
        .collect();
    let opts = ParallelOpts {
        threads: niao_parallel::available_threads(),
    };
    let par_iters = 20;
    let start = Instant::now();
    for _ in 0..par_iters {
        let _ = parallel_find(&docs, "$.payload.store.book[0].author", &opts).unwrap();
    }
    let ns = start.elapsed().as_nanos() as f64 / par_iters as f64;
    println!(
        "parallel_find {} docs x {} threads ({} iter): {:.0} ns/iter",
        docs.len(),
        opts.threads,
        par_iters,
        ns
    );

    let start = Instant::now();
    for _ in 0..par_iters {
        let _ = parallel_jmes(&docs, "payload.store.book | length(@)", &opts).unwrap();
    }
    let ns = start.elapsed().as_nanos() as f64 / par_iters as f64;
    println!(
        "parallel_jmes {} docs ({} iter): {:.0} ns/iter",
        docs.len(),
        par_iters,
        ns
    );
}
