//! Integration: sequential hello-world requests against the in-crate server.

use crate::server::{OutgoingResponse, Server};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

const REQUESTS: usize = 100;

fn read_response(stream: &mut TcpStream) -> Vec<u8> {
    let mut buf = Vec::with_capacity(512);
    let mut tmp = [0u8; 512];
    loop {
        let n = stream.read(&mut tmp).expect("read response");
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(header_end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let body_start = header_end + 4;
            let headers = std::str::from_utf8(&buf[..header_end]).unwrap_or("");
            let cl = headers
                .lines()
                .find_map(|line| {
                    let (k, v) = line.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if buf.len() >= body_start + cl {
                break;
            }
        }
    }
    buf
}

#[test]
fn sequential_hello_requests() {
    let server = Server::http("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    let handle = thread::spawn(move || {
        for _ in 0..REQUESTS {
            let req = server.recv().unwrap();
            req.respond(OutgoingResponse::from_string("ok")).unwrap();
        }
    });

    thread::sleep(Duration::from_millis(50));
    for _ in 0..REQUESTS {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let resp = read_response(&mut stream);
        assert!(
            resp.windows(2).any(|w| w == b"ok"),
            "response: {:?}",
            String::from_utf8_lossy(&resp)
        );
        thread::sleep(Duration::from_millis(2));
    }
    handle.join().unwrap();
}
