//! Micro-benchmarks for ngrpc hot paths (release mode).

use niao_grpc::{
    frame_message, unframe_all, unframe_one, CallOptions, Channel, GrpcServer, HandlerReply,
    MethodKind,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn bench(name: &str, iters: u32, mut f: impl FnMut()) {
    // warmup
    for _ in 0..iters / 10 {
        f();
    }
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = start.elapsed();
    let ns = elapsed.as_nanos() / iters as u128;
    let ops = if elapsed.as_secs_f64() > 0.0 {
        iters as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };
    println!("{name}: {ns} ns/op, {ops:.0} ops/sec ({iters} iters)");
}

fn main() {
    let payload = vec![0u8; 1024];

    bench("frame_1kib", 200_000, || {
        let _ = frame_message(&payload).unwrap();
    });

    let framed = frame_message(&payload).unwrap();
    bench("unframe_1kib", 200_000, || {
        let _ = unframe_one(&framed).unwrap();
    });

    let mut multi = Vec::new();
    for _ in 0..16 {
        multi.extend_from_slice(&frame_message(&payload).unwrap());
    }
    bench("unframe_all_16x1kib", 20_000, || {
        let _ = unframe_all(&multi).unwrap();
    });

    // Naive baseline: manual copy loop without BytesMut
    bench("naive_prefix_copy_1kib", 200_000, || {
        let mut out = Vec::with_capacity(5 + payload.len());
        out.push(0);
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(&payload);
        std::hint::black_box(out);
    });

    let server = GrpcServer::bind("127.0.0.1:0").expect("bind");
    let addr = server.addr();
    server
        .register(
            "/bench.Echo/Echo",
            MethodKind::Unary,
            Arc::new(|rpc| {
                HandlerReply::ok_bytes(rpc.messages.first().cloned().unwrap_or_default())
            }),
        )
        .unwrap();
    server.serve_bg().unwrap();
    std::thread::sleep(Duration::from_millis(80));

    let ch = Channel::connect(&addr, &CallOptions::default()).expect("connect");
    let small = b"hello";
    bench("unary_echo_5b", 500, || {
        let r = ch
            .unary("/bench.Echo/Echo", small, &CallOptions::default())
            .unwrap();
        assert!(r.status.is_ok());
    });

    let kib = vec![b'x'; 1024];
    bench("unary_echo_1kib", 500, || {
        let r = ch
            .unary("/bench.Echo/Echo", &kib, &CallOptions::default())
            .unwrap();
        assert!(r.status.is_ok());
    });

    server.stop();
    server.join_bg();
}
