//! Benchmark socket option round-trips: niao_io vs socket2.

use niao_io::{Domain, Socket, SocketOption, SocketOptionKind, Type};
use std::time::Instant;

const ITERS: u32 = 50_000;

fn bench_niao() -> f64 {
    let sock = Socket::new(Domain::Ipv4, Type::Stream, None).expect("niao socket");
    let start = Instant::now();
    for i in 0..ITERS {
        let on = i % 2 == 0;
        sock.set_opt(&SocketOption::Nodelay(on)).expect("set");
        let got = sock.get_opt(SocketOptionKind::Nodelay).expect("get");
        std::hint::black_box(got);
    }
    let secs = start.elapsed().as_secs_f64();
    ITERS as f64 / secs
}

fn bench_socket2() -> f64 {
    let sock = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)
        .expect("s2 socket");
    let start = Instant::now();
    for i in 0..ITERS {
        let on = i % 2 == 0;
        sock.set_nodelay(on).expect("set");
        let got = sock.nodelay().expect("get");
        std::hint::black_box(got);
    }
    let secs = start.elapsed().as_secs_f64();
    ITERS as f64 / secs
}

fn main() {
    println!("=== socket option bench (TCP_NODELAY set+get, {ITERS} iters) ===");
    let niao = bench_niao();
    let s2 = bench_socket2();
    println!("niao_io:  {niao:.0} ops/s");
    println!("socket2:  {s2:.0} ops/s");
    let ratio = niao / s2;
    println!("ratio (niao/socket2): {ratio:.2}x");
    if ratio >= 0.8 {
        println!("PASS: niao_io within 80% of socket2 throughput");
    } else {
        eprintln!("WARN: niao_io slower than 80% of socket2");
    }
}
