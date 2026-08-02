//! Micro-benchmarks for `niao_rpc` hot paths (release mode).
//! Run: cargo run -p niao_rpc --bin nrpc_bench --release

use niao_json_core::Value;
use niao_rpc::{
    decode, dispatch_str, encode, frame, unframe, FrameStyle, Id, Message, MethodTable, Request,
};
use std::time::Instant;

fn bench<F: FnMut() -> u64>(name: &str, mut f: F, iters: u64) {
    for _ in 0..3 {
        let _ = f();
    }
    let start = Instant::now();
    let n = f();
    let elapsed = start.elapsed();
    let mean_ns = elapsed.as_nanos() as f64 / iters as f64;
    let ops = (iters as f64) / elapsed.as_secs_f64();
    println!(
        "{name}: n={n} mean={mean_ns:.0} ns/op  {ops:.0} ops/s  total={} ms",
        elapsed.as_millis()
    );
}

fn main() {
    let sample = r#"{"jsonrpc":"2.0","method":"add","params":[1,2],"id":1}"#;

    bench(
        "decode request x100k",
        || {
            for _ in 0..100_000 {
                let _ = decode(sample).unwrap();
            }
            100_000
        },
        100_000,
    );

    let msg = Message::Request(Request::call(
        "add",
        Some(Value::array(vec![Value::int(1), Value::int(2)])),
        Id::Number(1),
    ));
    bench(
        "encode request x100k",
        || {
            for _ in 0..100_000 {
                let _ = encode(&msg);
            }
            100_000
        },
        100_000,
    );

    let mut table = MethodTable::new();
    table.register("add", |p| {
        let a = p
            .and_then(|v| match v {
                Value::Array(xs) => xs.first().and_then(|x| x.as_i64()),
                _ => None,
            })
            .unwrap_or(0);
        let b = p
            .and_then(|v| match v {
                Value::Array(xs) => xs.get(1).and_then(|x| x.as_i64()),
                _ => None,
            })
            .unwrap_or(0);
        Ok(Value::int(a + b))
    });
    bench(
        "dispatch add x100k",
        || {
            for _ in 0..100_000 {
                let _ = table.dispatch_str(sample);
            }
            100_000
        },
        100_000,
    );

    // Naive baseline: manual string find instead of full decode+dispatch.
    bench(
        "naive contains baseline x100k",
        || {
            for _ in 0..100_000 {
                let _ = sample.contains("\"method\":\"add\"") && sample.contains("\"id\":1");
            }
            100_000
        },
        100_000,
    );

    let framed = frame(&msg, FrameStyle::Ndjson);
    bench(
        "unframe ndjson x100k",
        || {
            for _ in 0..100_000 {
                let _ = unframe(&framed, FrameStyle::Ndjson).unwrap();
            }
            100_000
        },
        100_000,
    );

    let batch = format!(
        "[{0},{0},{0},{0},{0}]",
        r#"{"jsonrpc":"2.0","method":"add","params":[1,2],"id":1}"#
    );
    bench(
        "dispatch batch5 x20k",
        || {
            for _ in 0..20_000 {
                let _ = dispatch_str(&batch, |m, p| table.call(m, p));
            }
            20_000
        },
        20_000,
    );
}
