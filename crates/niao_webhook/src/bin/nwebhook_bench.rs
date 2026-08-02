//! Micro-benchmarks for `niao_webhook` hot paths.
//! Run: cargo run -p niao_webhook --bin nwebhook_bench --release

use niao_webhook::{make_headers, VerifyOptions, Webhook, WebhookOptions};
use std::time::Instant;

fn bench<F: FnMut() -> u64>(name: &str, mut f: F, iters: u64) {
    for _ in 0..3 {
        let _ = f();
    }
    let start = Instant::now();
    let n = f();
    let elapsed = start.elapsed();
    let mean_ns = elapsed.as_nanos() as f64 / iters as f64;
    let ops = if elapsed.as_secs_f64() > 0.0 {
        (iters as f64) / elapsed.as_secs_f64()
    } else {
        0.0
    };
    println!(
        "{name}: n={n} mean={mean_ns:.0} ns/op ops/sec={ops:.0} total={} ms",
        elapsed.as_millis()
    );
}

fn naive_sign(key: &[u8], msg_id: &str, ts: i64, payload: &str) -> String {
    // Intentionally allocate-heavy baseline for comparison.
    let data = format!("{msg_id}.{ts}.{payload}");
    let mac = niao_crypto::hmac_sha256(key, data.as_bytes());
    format!("v1,{}", niao_codec::base64::encode_standard(&mac))
}

fn main() {
    let secret = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
    let wh = Webhook::new(secret, WebhookOptions::default()).unwrap();
    let payload = r#"{"event":"invoice.paid","id":"inv_123","amount":1999,"currency":"usd"}"#;
    let msg_id = "msg_benchPayloadXXXXXXXXXXXX";
    let ts = 1_700_000_000i64;
    let key = niao_webhook::parse_secret(
        "MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw",
        niao_webhook::SecretFormat::Standard,
    )
    .unwrap();

    bench(
        "webhook.sign x200k",
        || {
            for i in 0..200_000 {
                let id = format!("{msg_id}{i}");
                let _ = wh.sign(&id, ts, payload).unwrap();
            }
            200_000
        },
        200_000,
    );

    let sig = wh.sign(msg_id, ts, payload).unwrap();
    let headers = make_headers(msg_id, ts, &sig);
    let opts = VerifyOptions {
        now: Some(ts),
        tolerance: 300,
        parse_json: true,
    };

    bench(
        "webhook.verify(+json) x100k",
        || {
            for _ in 0..100_000 {
                let _ = wh.verify(payload, &headers, &opts).unwrap();
            }
            100_000
        },
        100_000,
    );

    let opts_raw = VerifyOptions {
        now: Some(ts),
        tolerance: 300,
        parse_json: false,
    };

    bench(
        "webhook.verify_raw x200k",
        || {
            for _ in 0..200_000 {
                let _ = wh.verify_raw(payload, &headers, &opts_raw).unwrap();
            }
            200_000
        },
        200_000,
    );

    bench(
        "webhook.valid x200k",
        || {
            for _ in 0..200_000 {
                assert!(wh.valid(payload, &headers, &opts_raw));
            }
            200_000
        },
        200_000,
    );

    bench(
        "naive_sign(format!) x200k",
        || {
            for i in 0..200_000 {
                let id = format!("{msg_id}{i}");
                let _ = naive_sign(&key, &id, ts, payload);
            }
            200_000
        },
        200_000,
    );

    // Throughput on a larger body (~64 KiB).
    let big = "x".repeat(64 * 1024);
    let big_sig = wh.sign(msg_id, ts, &big).unwrap();
    let big_headers = make_headers(msg_id, ts, &big_sig);
    let nbytes = (big.len() * 20_000) as f64;
    let start = Instant::now();
    for _ in 0..20_000 {
        let _ = wh.verify_raw(&big, &big_headers, &opts_raw).unwrap();
    }
    let elapsed = start.elapsed().as_secs_f64();
    let mibs = (nbytes / (1024.0 * 1024.0)) / elapsed;
    println!(
        "webhook.verify_raw 64KiB x20k: {:.1} MiB/s total={} ms",
        mibs,
        (elapsed * 1000.0) as u64
    );
}
