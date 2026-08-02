//! Simple PostgreSQL round-trip latency benchmark.

use niao_db::postgres::{tls::NoTls, Client, Config};
use std::time::Instant;

fn main() {
    let url = std::env::var("NIAO_TEST_PG_URL").unwrap_or_else(|_| {
        eprintln!("Set NIAO_TEST_PG_URL for db_bench");
        std::process::exit(1);
    });
    let config: Config = url.parse().expect("parse url");
    let mut client = Client::connect(&config, NoTls).expect("connect");
    const N: u32 = 5000;
    let start = Instant::now();
    for _ in 0..N {
        client.query_one("SELECT 1", &[]).expect("query");
    }
    let secs = start.elapsed().as_secs_f64();
    println!("pg_simple_query_{N}: {:.0} qps", N as f64 / secs);
}
