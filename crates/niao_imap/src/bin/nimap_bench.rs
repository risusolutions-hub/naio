//! Micro-benchmarks for nimap hot paths (release mode).

use niao_imap::{
    format_message_set, imap_quote, mock, parse_headers, ConnectOptions, ImapClient, PopClient,
    PopConnectOptions,
};
use std::time::{Duration, Instant};

fn main() {
    let raw = "From: a@b.com\r\nSubject: Bench\r\n\r\nHello world body for parsing.\r\n";
    let n = 200_000usize;

    let t0 = Instant::now();
    for _ in 0..n {
        let _ = parse_headers(raw);
    }
    let e0 = t0.elapsed();
    println!(
        "parse_headers: {n} runs in {:.2} ms ({:.0} ns/op, {:.1} MB/s)",
        e0.as_secs_f64() * 1000.0,
        e0.as_nanos() as f64 / n as f64,
        (raw.len() * n) as f64 / e0.as_secs_f64() / 1_000_000.0
    );

    // naive baseline: split lines + HashMap insert without folding
    let t1 = Instant::now();
    for _ in 0..n {
        let mut map = std::collections::HashMap::new();
        for line in raw.lines() {
            if let Some((k, v)) = line.split_once(':') {
                map.insert(k.to_ascii_lowercase(), v.trim().to_string());
            }
        }
        let _ = map;
    }
    let e1 = t1.elapsed();
    println!(
        "naive_headers: {n} runs in {:.2} ms ({:.0} ns/op)",
        e1.as_secs_f64() * 1000.0,
        e1.as_nanos() as f64 / n as f64
    );

    let t2 = Instant::now();
    for i in 0..n {
        let _ = imap_quote(&format!("user-{i}@example.com"));
        let _ = format_message_set(&[1, 2, 3, 4, 5, 10, 11, 100]);
    }
    let e2 = t2.elapsed();
    println!(
        "quote+message_set: {n} runs in {:.2} ms ({:.0} ns/op)",
        e2.as_secs_f64() * 1000.0,
        e2.as_nanos() as f64 / n as f64
    );

    // Protocol round-trip throughput against mock (not nanos — msec/op)
    let server = mock::MockImapServer::start();
    let port = server.port();
    let opts = ConnectOptions {
        host: "127.0.0.1".into(),
        port,
        user: "u".into(),
        pass: "p".into(),
        tls: false,
        starttls: false,
        timeout: Duration::from_secs(5),
        mailbox: Some("INBOX".into()),
    };
    let mut c = ImapClient::connect(&opts).expect("connect");
    let rounds = 2_000usize;
    let t3 = Instant::now();
    for _ in 0..rounds {
        let _ = c.search("ALL", false).expect("search");
    }
    let e3 = t3.elapsed();
    println!(
        "imap_search_mock: {rounds} runs in {:.2} ms ({:.0} us/op, {:.0} ops/sec)",
        e3.as_secs_f64() * 1000.0,
        e3.as_secs_f64() * 1_000_000.0 / rounds as f64,
        rounds as f64 / e3.as_secs_f64()
    );
    c.logout().ok();
    server.shutdown();

    let pop = mock::MockPopServer::start();
    let pop_port = pop.port();
    let pop_opts = PopConnectOptions {
        host: "127.0.0.1".into(),
        port: pop_port,
        user: "u".into(),
        pass: "p".into(),
        tls: false,
        starttls: false,
        timeout: Duration::from_secs(5),
    };
    let mut pc = PopClient::connect(&pop_opts).expect("pop");
    let t4 = Instant::now();
    for _ in 0..rounds {
        let _ = pc.stat().expect("stat");
    }
    let e4 = t4.elapsed();
    println!(
        "pop_stat_mock: {rounds} runs in {:.2} ms ({:.0} us/op, {:.0} ops/sec)",
        e4.as_secs_f64() * 1000.0,
        e4.as_secs_f64() * 1_000_000.0 / rounds as f64,
        rounds as f64 / e4.as_secs_f64()
    );
    pc.quit().ok();
    pop.shutdown();
}
