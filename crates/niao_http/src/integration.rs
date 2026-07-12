//! Integration: 10k keep-alive style requests (sequential on one connection).

use crate::server::{OutgoingResponse, Server};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;

#[test]
fn ten_k_hello_requests() {
    let server = Server::http("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    let handle = thread::spawn(move || {
        for _ in 0..10_000 {
            let req = server.recv().unwrap();
            req.respond(OutgoingResponse::from_string("ok")).unwrap();
        }
    });

    let mut stream = TcpStream::connect(addr).unwrap();
    for _ in 0..10_000 {
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut buf = [0u8; 256];
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        assert!(buf[..n].windows(2).any(|w| w == b"ok"));
        stream = TcpStream::connect(addr).unwrap();
    }
    handle.join().unwrap();
}
