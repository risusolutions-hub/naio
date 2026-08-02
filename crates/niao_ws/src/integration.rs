//! Echo integration test.

use crate::{connect, Message, WsServer};
use std::thread;

#[test]
fn echo_client_server() {
    let server = WsServer::bind("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    let url = format!("ws://{addr}/");

    let handle = thread::spawn(move || {
        let mut ws = server.accept().unwrap();
        let msg = ws.read().unwrap();
        ws.send(msg).unwrap();
    });

    let (mut client, _) = connect(&url).unwrap();
    client.send(Message::Text("hello echo".into())).unwrap();
    match client.read().unwrap() {
        Message::Text(s) => assert_eq!(s, "hello echo"),
        other => panic!("unexpected {other:?}"),
    }
    handle.join().unwrap();
}
