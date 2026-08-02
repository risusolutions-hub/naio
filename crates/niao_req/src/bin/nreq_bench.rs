//! Micro-benchmarks for nreq hot paths (release mode).
use niao_http::{OutgoingResponse, Server};
use niao_req::{
    build_multipart, encode_form, execute, join_url, parse_set_cookie, prepare_url, MultipartPart,
    RequestOpts, Session,
};
use std::thread;
use std::time::Instant;

fn main() {
    let n = 200_000usize;

    let pairs: Vec<(String, String)> = (0..20)
        .map(|i| (format!("k{i}"), format!("value with spaces {i}")))
        .collect();
    let t0 = Instant::now();
    for _ in 0..n {
        let _ = encode_form(&pairs);
    }
    let e0 = t0.elapsed();
    println!(
        "encode_form (20 pairs): {n} runs in {:.2} ms ({:.0} ns/op, {:.1} ops/sec)",
        e0.as_secs_f64() * 1000.0,
        e0.as_nanos() as f64 / n as f64,
        n as f64 / e0.as_secs_f64()
    );

    // naive baseline: manual join without form_urlencode
    let t1 = Instant::now();
    for _ in 0..n {
        let mut s = String::new();
        for (i, (k, v)) in pairs.iter().enumerate() {
            if i > 0 {
                s.push('&');
            }
            s.push_str(k);
            s.push('=');
            s.push_str(v);
        }
        std::hint::black_box(s);
    }
    let e1 = t1.elapsed();
    println!(
        "naive form concat (baseline): {n} runs in {:.2} ms ({:.0} ns/op)",
        e1.as_secs_f64() * 1000.0,
        e1.as_nanos() as f64 / n as f64
    );

    let parts = vec![
        MultipartPart::field("title", b"hello world"),
        MultipartPart::file("f", "x.bin", vec![0u8; 1024], None),
    ];
    let t2 = Instant::now();
    for _ in 0..n / 10 {
        let _ = build_multipart(&parts, Some("BOUND")).unwrap();
    }
    let e2 = t2.elapsed();
    let n2 = n / 10;
    println!(
        "build_multipart (1KB file): {n2} runs in {:.2} ms ({:.0} ns/op)",
        e2.as_secs_f64() * 1000.0,
        e2.as_nanos() as f64 / n2 as f64
    );

    let t3 = Instant::now();
    for _ in 0..n {
        let _ = parse_set_cookie("session=abc; Path=/; HttpOnly; Secure").unwrap();
    }
    let e3 = t3.elapsed();
    println!(
        "parse_set_cookie: {n} runs in {:.2} ms ({:.0} ns/op)",
        e3.as_secs_f64() * 1000.0,
        e3.as_nanos() as f64 / n as f64
    );

    let t4 = Instant::now();
    for _ in 0..n {
        let _ = prepare_url(
            "https://api.example.com/v1",
            Some("users"),
            &[("q".into(), "niao".into()), ("page".into(), "1".into())],
        )
        .unwrap();
    }
    let e4 = t4.elapsed();
    println!(
        "prepare_url: {n} runs in {:.2} ms ({:.0} ns/op)",
        e4.as_secs_f64() * 1000.0,
        e4.as_nanos() as f64 / n as f64
    );

    let t5 = Instant::now();
    for _ in 0..n {
        let _ = join_url("https://example.com/api/", "v2/items").unwrap();
    }
    let e5 = t5.elapsed();
    println!(
        "join_url: {n} runs in {:.2} ms ({:.0} ns/op)",
        e5.as_secs_f64() * 1000.0,
        e5.as_nanos() as f64 / n as f64
    );

    // Live HTTP roundtrip microbench (blocking accept per request)
    let server = Server::http("127.0.0.1:0").expect("bind");
    let addr = server.local_addr().unwrap();
    let url = format!("http://{addr}/bench");
    let n_http = 200usize;
    let handle = thread::spawn(move || {
        for _ in 0..n_http {
            if let Ok(req) = server.recv() {
                let _ = req.respond(OutgoingResponse::from_string("ok"));
            }
        }
    });
    thread::sleep(std::time::Duration::from_millis(10));
    let mut session = Session::new();
    let t6 = Instant::now();
    for _ in 0..n_http {
        let r = execute("GET", &url, &mut session, &RequestOpts::default());
        if let Err(e) = r {
            eprintln!("warn: GET failed: {e}");
            break;
        }
    }
    let e6 = t6.elapsed();
    let _ = handle.join();
    let bytes = 2u64 * n_http as u64; // "ok"
    let mbps = (bytes as f64 / (1024.0 * 1024.0)) / e6.as_secs_f64().max(1e-9);
    println!(
        "local GET roundtrip: {n_http} runs in {:.2} ms ({:.0} ns/op, {:.2} ops/sec, body ~{:.4} MB/s)",
        e6.as_secs_f64() * 1000.0,
        e6.as_nanos() as f64 / n_http as f64,
        n_http as f64 / e6.as_secs_f64().max(1e-9),
        mbps
    );
}
