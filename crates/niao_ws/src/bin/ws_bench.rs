//! WebSocket text message throughput (single persistent connection).

use niao_ws::{connect, Message, WsServer};
use std::thread;
use std::time::Instant;

const MSGS: u32 = 100_000;

fn main() {
    let server = WsServer::bind("127.0.0.1:0").expect("bind");
    let addr = server.local_addr().expect("addr");
    let url = format!("ws://{addr}/");

    thread::spawn(move || {
        let mut ws = server.accept().expect("accept");
        for _ in 0..MSGS {
            let msg = ws.read().expect("read");
            ws.send(msg).expect("send");
        }
    });

    let (mut client, _) = connect(&url).expect("connect");
    let start = Instant::now();
    for _ in 0..MSGS {
        client.send(Message::Text("ping".into())).expect("send");
        let _ = client.read().expect("recv");
    }
    let secs = start.elapsed().as_secs_f64();
    println!("ws_echo_{MSGS}: {:.0} msg/s", MSGS as f64 / secs);
}
