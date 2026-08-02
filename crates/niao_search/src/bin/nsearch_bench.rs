//! Micro-benchmarks for nsearch hot paths (release mode).
use niao_http::{OutgoingResponse, Server};
use niao_search::{
    encode_params, es_bulk_ndjson, es_query, extract_hits, join_url, meili_filter, prepare_url,
    search, ts_filter, Auth, BulkOp, Client, Engine, EsQueryOpts, SearchOpts,
};
use std::thread;
use std::time::Instant;

fn main() {
    let n = 200_000usize;

    let t0 = Instant::now();
    for _ in 0..n {
        let _ = es_query(&EsQueryOpts {
            q: Some("niao search engine".into()),
            fields: vec!["title".into(), "body".into()],
            size: Some(20),
            from: Some(0),
            ..Default::default()
        });
    }
    let e0 = t0.elapsed();
    println!(
        "es_query (multi_match): {n} runs in {:.2} ms ({:.0} ns/op, {:.1} ops/sec)",
        e0.as_secs_f64() * 1000.0,
        e0.as_nanos() as f64 / n as f64,
        n as f64 / e0.as_secs_f64()
    );

    // naive baseline: format! string concat without JSON escaping
    let t1 = Instant::now();
    for _ in 0..n {
        let s = format!(
            r#"{{"query":{{"multi_match":{{"query":"{}","fields":["title","body"]}}}},"size":20}}"#,
            "niao search engine"
        );
        std::hint::black_box(s);
    }
    let e1 = t1.elapsed();
    println!(
        "naive format! DSL (baseline): {n} runs in {:.2} ms ({:.0} ns/op, {:.1}x vs es_query)",
        e1.as_secs_f64() * 1000.0,
        e1.as_nanos() as f64 / n as f64,
        e0.as_secs_f64() / e1.as_secs_f64().max(1e-12)
    );

    let ops = vec![
        BulkOp {
            action: "index".into(),
            index: "docs".into(),
            id: Some("1".into()),
            doc_json: Some(r#"{"title":"hello","tags":["a","b"]}"#.into()),
        },
        BulkOp {
            action: "index".into(),
            index: "docs".into(),
            id: Some("2".into()),
            doc_json: Some(r#"{"title":"world"}"#.into()),
        },
    ];
    let t2 = Instant::now();
    for _ in 0..n / 10 {
        let _ = es_bulk_ndjson(&ops).unwrap();
    }
    let e2 = t2.elapsed();
    let n2 = n / 10;
    println!(
        "es_bulk_ndjson (2 ops): {n2} runs in {:.2} ms ({:.0} ns/op, {:.1} ops/sec)",
        e2.as_secs_f64() * 1000.0,
        e2.as_nanos() as f64 / n2 as f64,
        n2 as f64 / e2.as_secs_f64()
    );

    let parts = vec![
        "genre = action".into(),
        "year > 2000".into(),
        "rating >= 4".into(),
    ];
    let t3 = Instant::now();
    for _ in 0..n {
        let _ = meili_filter(&parts);
        let _ = ts_filter(&parts);
    }
    let e3 = t3.elapsed();
    println!(
        "meili_filter+ts_filter: {n} runs in {:.2} ms ({:.0} ns/op)",
        e3.as_secs_f64() * 1000.0,
        e3.as_nanos() as f64 / n as f64
    );

    let params = vec![
        ("q".into(), "hello world".into()),
        ("page".into(), "1".into()),
        ("lang".into(), "en".into()),
    ];
    let t4 = Instant::now();
    for _ in 0..n {
        let _ = encode_params(&params);
        let _ = prepare_url("http://localhost:7700/indexes/movies/search", None, &params).unwrap();
        let _ = join_url("http://localhost:9200/", "docs/_search").unwrap();
    }
    let e4 = t4.elapsed();
    println!(
        "encode_params+prepare_url+join: {n} runs in {:.2} ms ({:.0} ns/op)",
        e4.as_secs_f64() * 1000.0,
        e4.as_nanos() as f64 / n as f64
    );

    let body = r#"{"hits":{"hits":[{"_source":{"t":1}},{"_source":{"t":2}},{"_source":{"t":3}}]}}"#;
    let t5 = Instant::now();
    for _ in 0..n / 5 {
        let _ = extract_hits("elasticsearch", body).unwrap();
    }
    let e5 = t5.elapsed();
    let n5 = n / 5;
    println!(
        "extract_hits (3 hits): {n5} runs in {:.2} ms ({:.0} ns/op, {:.1} ops/sec)",
        e5.as_secs_f64() * 1000.0,
        e5.as_nanos() as f64 / n5 as f64,
        n5 as f64 / e5.as_secs_f64()
    );

    // Live local HTTP search roundtrip
    let server = Server::http("127.0.0.1:0").expect("bind");
    let addr = server.local_addr().unwrap();
    let url = format!("http://{addr}");
    let n_http = 200usize;
    let handle = thread::spawn(move || {
        for _ in 0..n_http {
            if let Ok(req) = server.recv() {
                let body = r#"{"hits":{"hits":[{"_source":{"ok":true}}]}}"#;
                let _ = req.respond(OutgoingResponse::from_string(body));
            }
        }
    });
    thread::sleep(std::time::Duration::from_millis(15));
    let client = Client::new(Engine::Elasticsearch, url, Auth::None, 5_000);
    let t6 = Instant::now();
    for _ in 0..n_http {
        let r = search(
            &client,
            &SearchOpts {
                index: "bench".into(),
                q: Some("x".into()),
                limit: Some(1),
                ..Default::default()
            },
        );
        if let Err(e) = r {
            eprintln!("warn: search failed: {e}");
            break;
        }
    }
    let e6 = t6.elapsed();
    let _ = handle.join();
    println!(
        "local ES search roundtrip: {n_http} runs in {:.2} ms ({:.0} ns/op, {:.2} ops/sec)",
        e6.as_secs_f64() * 1000.0,
        e6.as_nanos() as f64 / n_http as f64,
        n_http as f64 / e6.as_secs_f64().max(1e-9)
    );
}
