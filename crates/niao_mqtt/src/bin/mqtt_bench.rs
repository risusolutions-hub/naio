//! Release micro-benchmarks for `niao_mqtt` codec hot paths.

use niao_mqtt::{
    decode_packet, encode_connect, encode_publish, topic_matches, ConnectOptions, PublishPacket,
};
use std::time::Instant;

fn bench(name: &str, iters: u32, mut f: impl FnMut()) {
    for _ in 0..50 {
        f();
    }
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = start.elapsed();
    let ns = elapsed.as_nanos() / iters as u128;
    let ops = if ns > 0 {
        1_000_000_000u128 / ns
    } else {
        u128::MAX
    };
    println!("{name}: {ns} ns/op  (~{ops} ops/sec)");
}

fn main() {
    let opts = ConnectOptions {
        client_id: "bench-client".into(),
        username: Some("user".into()),
        password: Some(b"secret".to_vec()),
        will: Some(niao_mqtt::Will {
            topic: "status/edge".into(),
            payload: b"offline".to_vec(),
            qos: 1,
            retain: true,
        }),
        ..Default::default()
    };
    let connect_bytes = encode_connect(&opts).unwrap();

    let pubpkt = PublishPacket {
        topic: "fleet/device-42/telemetry".into(),
        payload: vec![0u8; 1024],
        qos: 1,
        retain: false,
        dup: false,
        packet_id: Some(7),
    };
    let publish_bytes = encode_publish(&pubpkt).unwrap();

    println!("niao_mqtt release benchmarks");
    println!("connect packet size: {} bytes", connect_bytes.len());
    println!("publish 1KiB packet size: {} bytes", publish_bytes.len());

    bench("encode_connect", 100_000, || {
        let _ = encode_connect(&opts).unwrap();
    });
    bench("decode_connect", 100_000, || {
        let _ = decode_packet(&connect_bytes).unwrap();
    });
    bench("encode_publish_1KiB", 100_000, || {
        let _ = encode_publish(&pubpkt).unwrap();
    });
    bench("decode_publish_1KiB", 100_000, || {
        let _ = decode_packet(&publish_bytes).unwrap();
    });

    // Naive baseline: rebuild publish by hand with string format + extend
    let topic = "fleet/device-42/telemetry";
    let payload = vec![0u8; 1024];
    bench("naive_publish_assemble", 100_000, || {
        let mut v = Vec::new();
        v.push(0x32); // PUBLISH qos1
        let body_len = 2 + topic.len() + 2 + payload.len();
        v.push(body_len as u8);
        v.push((topic.len() >> 8) as u8);
        v.push((topic.len() & 0xff) as u8);
        v.extend_from_slice(topic.as_bytes());
        v.push(0);
        v.push(7);
        v.extend_from_slice(&payload);
        std::hint::black_box(v);
    });

    bench("topic_matches_hot", 500_000, || {
        let _ = topic_matches("fleet/+/telemetry/#", "fleet/device-42/telemetry/cpu");
    });
}
