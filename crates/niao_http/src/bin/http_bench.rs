//! Hello-world server throughput benchmark.

use niao_http::{OutgoingResponse, Server};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

const REQUESTS: usize = 10_000;

fn main() {
    let server = Server::http("127.0.0.1:0").expect("bind");
    let addr = server.local_addr().expect("addr");
    let done = Arc::new(AtomicUsize::new(0));
    let done_srv = Arc::clone(&done);

    thread::spawn(move || {
        while done_srv.load(Ordering::Relaxed) < REQUESTS {
            if let Ok(req) = server.recv() {
                let _ = req.respond(OutgoingResponse::from_string("hello"));
                done_srv.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    let start = Instant::now();
    for _ in 0..REQUESTS {
        let mut s = TcpStream::connect(addr).expect("connect");
        s.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut buf = [0u8; 128];
        let _ = s.read(&mut buf).unwrap();
    }
    let secs = start.elapsed().as_secs_f64();
    println!(
        "http_hello_{REQUESTS}: {:.0} req/s",
        REQUESTS as f64 / secs
    );
}
