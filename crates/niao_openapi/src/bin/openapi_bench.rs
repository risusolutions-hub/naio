//! Micro-benchmarks for nopenapi hot paths (release mode).
use niao_openapi::{
    client_stub_str, from_routes, parallel_validate, sample_routes, validate, OpenApiDoc,
};
use serde_json::json;
use std::time::Instant;

fn bench_from_routes(n_routes: usize, iters: usize) -> f64 {
    let routes = sample_routes(n_routes);
    let info = json!({"title": "Bench", "version": "1.0.0"});
    let info = info.as_object().unwrap();
    let start = Instant::now();
    let mut acc = 0usize;
    for _ in 0..iters {
        let doc = from_routes(&routes, Some(info), None).unwrap();
        acc += doc.paths().len();
    }
    let _ = acc;
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_to_json(n_routes: usize, iters: usize) -> f64 {
    let routes = sample_routes(n_routes);
    let info = json!({"title": "Bench", "version": "1.0.0"});
    let doc = from_routes(&routes, info.as_object(), None).unwrap();
    let start = Instant::now();
    let mut bytes = 0usize;
    for _ in 0..iters {
        bytes += doc.to_json(false).unwrap().len();
    }
    let _ = bytes;
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_validate(n_routes: usize, iters: usize) -> f64 {
    let routes = sample_routes(n_routes);
    let info = json!({"title": "Bench", "version": "1.0.0"});
    let doc = from_routes(&routes, info.as_object(), None).unwrap();
    let start = Instant::now();
    let mut ok = 0usize;
    for _ in 0..iters {
        if validate(&doc).unwrap().ok {
            ok += 1;
        }
    }
    let _ = ok;
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_client_stub(n_routes: usize, iters: usize) -> f64 {
    let routes = sample_routes(n_routes);
    let info = json!({"title": "Bench", "version": "1.0.0"});
    let doc = from_routes(&routes, info.as_object(), None).unwrap();
    let start = Instant::now();
    let mut chars = 0usize;
    for _ in 0..iters {
        chars += client_stub_str(&doc, None).unwrap().len();
    }
    let _ = chars;
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn bench_parse(n_routes: usize, iters: usize) -> f64 {
    let routes = sample_routes(n_routes);
    let info = json!({"title": "Bench", "version": "1.0.0"});
    let doc = from_routes(&routes, info.as_object(), None).unwrap();
    let s = doc.to_json(false).unwrap();
    let start = Instant::now();
    let mut acc = 0usize;
    for _ in 0..iters {
        acc += OpenApiDoc::parse_str(&s).unwrap().paths().len();
    }
    let _ = acc;
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn naive_build_json(n_routes: usize) -> String {
    // Intentionally naive string concat baseline
    let mut s =
        String::from(r#"{"openapi":"3.1.0","info":{"title":"Bench","version":"1.0.0"},"paths":{"#);
    for i in 0..n_routes {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            r#""/items/{i}":{{"get":{{"responses":{{"200":{{"description":"OK"}}}}}}}}"#
        ));
    }
    s.push_str("}}");
    s
}

fn bench_naive(n_routes: usize, iters: usize) -> f64 {
    let start = Instant::now();
    let mut bytes = 0usize;
    for _ in 0..iters {
        bytes += naive_build_json(n_routes).len();
    }
    let _ = bytes;
    start.elapsed().as_nanos() as f64 / iters as f64
}

fn main() {
    let n = 200;
    let iters = 500;
    println!("fixture: {n} routes, {iters} iters (release micro-bench)");

    for _ in 0..3 {
        let _ = from_routes(
            &sample_routes(10),
            json!({"title":"W","version":"1"}).as_object(),
            None,
        );
    }

    let fr = bench_from_routes(n, iters);
    let tj = bench_to_json(n, iters);
    let va = bench_validate(n, iters);
    let cs = bench_client_stub(n, iters.min(100));
    let pa = bench_parse(n, iters);
    let nv = bench_naive(n, iters);

    let docs: Vec<_> = (0..32)
        .map(|_| {
            from_routes(
                &sample_routes(50),
                json!({"title": "P", "version": "1"}).as_object(),
                None,
            )
            .unwrap()
        })
        .collect();
    let start = Instant::now();
    let reports = parallel_validate(&docs, 0);
    let parallel_ns = start.elapsed().as_nanos() as f64 / docs.len() as f64;
    assert!(reports.iter().all(|r| r.ok));

    println!(
        "from_routes({n}):     {fr:.1} ns/op  ({:.0} ops/sec)",
        1e9 / fr
    );
    println!(
        "to_json({n}):         {tj:.1} ns/op  ({:.0} ops/sec)",
        1e9 / tj
    );
    println!(
        "validate({n}):        {va:.1} ns/op  ({:.0} ops/sec)",
        1e9 / va
    );
    println!(
        "parse({n}):           {pa:.1} ns/op  ({:.0} ops/sec)",
        1e9 / pa
    );
    println!(
        "client_stub({n}):     {cs:.1} ns/op  ({:.0} ops/sec)",
        1e9 / cs
    );
    println!("naive_string({n}):    {nv:.1} ns/op  (baseline)");
    println!("parallel_validate:   {parallel_ns:.1} ns/doc  (32 docs × 50 routes)");

    // Throughput estimate for JSON emit
    let doc = from_routes(
        &sample_routes(n),
        json!({"title": "Bench", "version": "1"}).as_object(),
        None,
    )
    .unwrap();
    let bytes = doc.to_json(false).unwrap().len() as f64;
    let mb_s = (bytes / (tj / 1e9)) / (1024.0 * 1024.0);
    println!("to_json throughput:  {mb_s:.1} MB/s  ({bytes:.0} bytes/spec)");
}
