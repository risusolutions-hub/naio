//! Micro-benchmarks for `niao_sign` hot paths.
//! Run: cargo run -p niao_sign --bin nsign_bench --release

use niao_json_core::{parse, Value};
use niao_sign::{
    sign_cookie_value, sign_url, Serializer, SerializerKind, SerializerOptions, Signer,
    SignerConfig, TimestampSigner,
};
use std::time::Instant;

fn bench<F: FnMut() -> u64>(name: &str, mut f: F, iters: u64) {
    for _ in 0..3 {
        f();
    }
    let start = Instant::now();
    let n = f();
    let elapsed = start.elapsed();
    let mean_ns = elapsed.as_nanos() as f64 / iters as f64;
    println!(
        "{name}: n={n} mean={mean_ns:.0} ns total={} ms",
        elapsed.as_millis()
    );
}

fn main() {
    let secret = b"bench-secret-key-32bytes-long!!";
    let signer = Signer::new(secret, SignerConfig::default()).unwrap();
    let ts_signer = TimestampSigner::new(secret, SignerConfig::default()).unwrap();
    let json_ser = Serializer::new(secret, SignerConfig::default(), SerializerKind::Json).unwrap();
    let url_ser =
        Serializer::timed(secret, SignerConfig::default(), SerializerKind::UrlSafe).unwrap();
    let payload = parse(r#"{"user_id":42,"role":"admin","email":"u@example.com"}"#).unwrap();
    let opts = SerializerOptions::default();

    bench(
        "signer.sign x100k",
        || {
            for i in 0..100_000 {
                let _ = signer.sign(&format!("payload-{i}")).unwrap();
            }
            100_000
        },
        100_000,
    );

    bench(
        "signer.unsign x100k",
        || {
            let tokens: Vec<String> = (0..1000)
                .map(|i| signer.sign(&format!("payload-{i}")).unwrap())
                .collect();
            for _ in 0..100 {
                for t in &tokens {
                    let _ = signer.unsign(t).unwrap();
                }
            }
            100_000
        },
        100_000,
    );

    bench(
        "timestamp.sign x100k",
        || {
            for i in 0..100_000 {
                let _ = ts_signer.sign(&format!("t-{i}")).unwrap();
            }
            100_000
        },
        100_000,
    );

    bench(
        "timestamp.unsign x100k",
        || {
            let tokens: Vec<String> = (0..1000)
                .map(|i| ts_signer.sign(&format!("t-{i}")).unwrap())
                .collect();
            for _ in 0..100 {
                for t in &tokens {
                    let _ = ts_signer.unsign(t, Some(3600)).unwrap();
                }
            }
            100_000
        },
        100_000,
    );

    bench(
        "serializer.dumps_json x50k",
        || {
            for _ in 0..50_000 {
                let _ = json_ser.dumps_json(&payload).unwrap();
            }
            50_000
        },
        50_000,
    );

    bench(
        "serializer.loads_json x50k",
        || {
            let tok = json_ser.dumps_json(&payload).unwrap();
            for _ in 0..50_000 {
                let _ = json_ser.loads_json(&tok, None).unwrap();
            }
            50_000
        },
        50_000,
    );

    bench(
        "url_safe.dumps_json x50k",
        || {
            for _ in 0..50_000 {
                let _ = url_ser.dumps_json(&payload).unwrap();
            }
            50_000
        },
        50_000,
    );

    bench(
        "url_safe.loads_json x50k",
        || {
            let tok = url_ser.dumps_json(&payload).unwrap();
            for _ in 0..50_000 {
                let _ = url_ser.loads_json(&tok, Some(86400)).unwrap();
            }
            50_000
        },
        50_000,
    );

    bench(
        "sign_url x20k",
        || {
            for i in 0..20_000 {
                let v = parse(&format!(r#"{{"id":{i}}}"#)).unwrap();
                let _ = sign_url("https://app.test/action", &v, secret, &opts, "token").unwrap();
            }
            20_000
        },
        20_000,
    );

    bench(
        "sign_cookie x20k",
        || {
            for i in 0..20_000 {
                let v = parse(&format!(r#"{{"sid":{i}}}"#)).unwrap();
                let _ = sign_cookie_value("session", &v, secret, &opts).unwrap();
            }
            20_000
        },
        20_000,
    );
}
