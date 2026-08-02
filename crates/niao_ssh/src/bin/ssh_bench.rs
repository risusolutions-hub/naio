//! Release-mode micro-benchmarks for niao_ssh.
//! Run: cargo run -p niao_ssh --release --bin ssh_bench

use niao_ssh::key_fingerprint;
use rand_core::OsRng;
use russh::keys::PrivateKey;
use std::time::Instant;

fn main() {
    println!("niao_ssh release benchmarks");
    println!(
        "os={} arch={}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );

    // --- key_fingerprint ---
    let key = PrivateKey::random(&mut OsRng, russh::keys::Algorithm::Ed25519).unwrap();
    let pem = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .unwrap()
        .to_string();
    let warmup = 50usize;
    let iters = 500usize;
    for _ in 0..warmup {
        let _ = key_fingerprint(&pem, false, None).unwrap();
    }
    let t0 = Instant::now();
    for _ in 0..iters {
        let _ = key_fingerprint(&pem, false, None).unwrap();
    }
    let elapsed = t0.elapsed();
    let ns = elapsed.as_nanos() as f64 / iters as f64;
    let ops = 1e9_f64 / ns;
    println!("key_fingerprint: {ops:.0} ops/sec, {ns:.0} ns/op (n={iters})");

    // --- connect refused (negative path latency) ---
    {
        use niao_ssh::{connect, ConnectConfig};
        let cfg = ConnectConfig {
            host: "127.0.0.1".into(),
            port: 1, // typically closed
            user: "x".into(),
            password: Some("y".into()),
            key_path: None,
            key_data: None,
            passphrase: None,
            agent: false,
            timeout_ms: Some(200),
        };
        let warmup = 2usize;
        let iters = 10usize;
        for _ in 0..warmup {
            let _ = connect(&cfg);
        }
        let t0 = Instant::now();
        for _ in 0..iters {
            let _ = connect(&cfg);
        }
        let elapsed = t0.elapsed();
        let ns = elapsed.as_nanos() as f64 / iters as f64;
        println!("connect_refused(port1,200ms): {:.0} ns/op (n={iters})", ns);
    }

    // Live round-trip is covered by `cargo test -p niao_ssh --release` integration tests.
    println!(
        "note: live connect+exec / sftp MB/s measured via cargo test --release nssh_live_bench"
    );
}
