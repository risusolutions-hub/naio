//! Micro-benchmarks for njwt hot paths.
//! Run: cargo run -p niao_jwt --bin njwt_bench --release

use niao_json_core::parse;
use niao_jwt::{sign_hs256_default, verify, Key, VerifyOptions};
use std::time::Instant;

const SECRET: &[u8] = b"benchmark-secret-key-32bytes!!";
const JWT_IO: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
    eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWUsImlhdCI6MTUxNjIzOTAyMn0.\
    reGQzG3OKdoIMWLDKOZ4TICJit3EW69cQE72E2CfzRE";

fn bench<F: Fn() -> usize>(name: &str, f: F, warmup: u32, iters: u32) {
    for _ in 0..warmup {
        let _ = f();
    }
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let t0 = Instant::now();
        let n = f();
        samples.push(t0.elapsed().as_nanos() as u64);
        let _ = n;
    }
    samples.sort_unstable();
    let mean = samples.iter().sum::<u64>() / iters as u64;
    let p50 = samples[samples.len() / 2];
    let p95 = samples[(samples.len() as f64 * 0.95) as usize];
    println!("{name}: mean={mean} ns p50={p50} ns p95={p95} ns (n={iters})");
}

fn main() {
    let claims = parse(r#"{"sub":"user-42","roles":["admin"],"exp":9999999999}"#).unwrap();
    let token = sign_hs256_default(&claims, SECRET).expect("sign");
    let key = Key::from_secret(SECRET, Some("HS256")).expect("key");
    let opts = VerifyOptions {
        validate_exp: false,
        ..Default::default()
    };
    let jwt_io_key = Key::from_secret(b"your-256-bit-secret", Some("HS256")).expect("key");

    let warmup = 5u32;
    let iters = 50u32;

    bench(
        "sign HS256 (niao_crypto fast path)",
        || sign_hs256_default(&claims, SECRET).map(|_| 1).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "verify HS256 fast path",
        || verify(&token, &key, &opts).map(|_| 1).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "verify jwt.io HS256 vector",
        || verify(JWT_IO, &jwt_io_key, &opts).map(|_| 1).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "decode_unverified",
        || niao_jwt::decode_unverified(&token).map(|_| 1).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "sign+verify roundtrip",
        || {
            let t = sign_hs256_default(&claims, SECRET).unwrap();
            verify(&t, &key, &opts).map(|_| 1).unwrap_or(0)
        },
        warmup,
        iters,
    );
}
