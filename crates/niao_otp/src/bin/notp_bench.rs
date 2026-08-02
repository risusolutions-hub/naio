//! Micro-benchmarks for `niao_otp` hot paths.
//! Run: cargo run -p niao_otp --bin notp_bench --release

use niao_otp::{
    hotp_at_bulk, totp_at, totp_at_bulk, Digest, Hotp, Totp, DEFAULT_DIGITS, DEFAULT_INTERVAL,
};
use std::time::Instant;

const SECRET: &str = "JBSWY3DPEHPK3PXP";

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
    let hotp = Hotp::new(SECRET, DEFAULT_DIGITS, Digest::Sha1).expect("hotp");
    let totp = Totp::new(SECRET, DEFAULT_DIGITS, DEFAULT_INTERVAL, Digest::Sha1).expect("totp");

    bench(
        "hotp.at x100k",
        || {
            for i in 0..100_000u64 {
                let _ = hotp.at(i);
            }
            100_000
        },
        100_000,
    );

    bench(
        "totp.at x100k",
        || {
            for i in 0..100_000u64 {
                let _ = totp.at(i * 30);
            }
            100_000
        },
        100_000,
    );

    bench(
        "totp_at flat x100k",
        || {
            for i in 0..100_000u64 {
                let _ = totp_at(
                    SECRET,
                    i * 30,
                    DEFAULT_DIGITS,
                    DEFAULT_INTERVAL,
                    Digest::Sha1,
                )
                .unwrap();
            }
            100_000
        },
        100_000,
    );

    bench(
        "hotp verify x50k",
        || {
            for i in 0..50_000u64 {
                let code = hotp.at(i);
                let _ = hotp.verify(&code, i);
            }
            50_000
        },
        50_000,
    );

    bench(
        "totp verify window=1 x50k",
        || {
            let t = 1_111_111_111u64;
            let code = totp.at(t);
            for _ in 0..50_000 {
                let _ = totp.verify(&code, t, 1);
            }
            50_000
        },
        50_000,
    );

    let counters: Vec<u64> = (0..10_000).collect();
    bench(
        "hotp_at_bulk 10k x10",
        || {
            for _ in 0..10 {
                let _ = hotp_at_bulk(SECRET, &counters, DEFAULT_DIGITS, Digest::Sha1).unwrap();
            }
            100_000
        },
        100_000,
    );

    let times: Vec<u64> = (0..10_000).map(|i| i * 30).collect();
    bench(
        "totp_at_bulk 10k x10",
        || {
            for _ in 0..10 {
                let _ = totp_at_bulk(
                    SECRET,
                    &times,
                    DEFAULT_DIGITS,
                    DEFAULT_INTERVAL,
                    Digest::Sha1,
                )
                .unwrap();
            }
            100_000
        },
        100_000,
    );
}
