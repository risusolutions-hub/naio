//! Micro-benchmarks for `niao_pass` hot paths.
//! Run: cargo run -p niao_pass --bin npass_bench --release

use niao_pass::{
    argon2, bcrypt, check_strength, generate, identify, scrypt, verify_password, Argon2Opts,
    CryptContext, ScryptOpts,
};
use std::time::Instant;

fn bench<F: FnMut() -> u64>(name: &str, mut f: F, iters: u64) {
    for _ in 0..2 {
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
    let argon_fast = Argon2Opts {
        memory_kib: 8_192,
        time_cost: 1,
        parallelism: 1,
    };
    let scrypt_fast = ScryptOpts {
        log_n: 10,
        r: 8,
        p: 1,
    };

    let argon_hash = argon2::hash_password("benchmark-secret", &argon_fast).unwrap();
    let bcrypt_hash = bcrypt::hash_password("benchmark-secret", 4).unwrap();
    let scrypt_hash = scrypt::hash_password("benchmark-secret", &scrypt_fast).unwrap();

    bench(
        "argon2id hash (m=8192,t=1)",
        || {
            let _ = argon2::hash_password("benchmark-secret", &argon_fast).unwrap();
            100
        },
        100,
    );

    bench(
        "argon2id verify",
        || {
            for _ in 0..1_000 {
                let _ = argon2::verify_password("benchmark-secret", &argon_hash).unwrap();
            }
            1_000
        },
        1_000,
    );

    bench(
        "bcrypt hash (cost=4)",
        || {
            let _ = bcrypt::hash_password("benchmark-secret", 4).unwrap();
            100
        },
        100,
    );

    bench(
        "bcrypt verify",
        || {
            for _ in 0..1_000 {
                let _ = bcrypt::verify_password("benchmark-secret", &bcrypt_hash).unwrap();
            }
            1_000
        },
        1_000,
    );

    bench(
        "scrypt hash (ln=10)",
        || {
            let _ = scrypt::hash_password("benchmark-secret", &scrypt_fast).unwrap();
            100
        },
        100,
    );

    bench(
        "scrypt verify",
        || {
            for _ in 0..1_000 {
                let _ = scrypt::verify_password("benchmark-secret", &scrypt_hash).unwrap();
            }
            1_000
        },
        1_000,
    );

    bench(
        "auto verify (argon2id)",
        || {
            for _ in 0..1_000 {
                let _ = verify_password("benchmark-secret", &argon_hash).unwrap();
            }
            1_000
        },
        1_000,
    );

    bench(
        "identify hash",
        || {
            for _ in 0..100_000 {
                let _ = identify(&argon_hash);
            }
            100_000
        },
        100_000,
    );

    bench(
        "check_strength",
        || {
            for _ in 0..50_000 {
                let _ = check_strength("Tr0ub4dor&3Extra!").unwrap();
            }
            50_000
        },
        50_000,
    );

    bench(
        "generate password len=16",
        || {
            for _ in 0..10_000 {
                let _ = generate(16, None).unwrap();
            }
            10_000
        },
        10_000,
    );

    let ctx = CryptContext::default();
    bench(
        "context verify_and_update (no rehash)",
        || {
            for _ in 0..500 {
                let _ = ctx
                    .verify_and_update("benchmark-secret", &argon_hash)
                    .unwrap();
            }
            500
        },
        500,
    );
}
