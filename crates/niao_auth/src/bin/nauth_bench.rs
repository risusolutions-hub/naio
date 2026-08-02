//! Micro-benchmarks for `niao_auth` hot paths.
//! Run: cargo run -p niao_auth --bin nauth_bench --release

use niao_auth::{
    allows, compare, context_from_opts, csrf_issue, csrf_validate, expand_roles, generate_token,
    load_session, sign_session, Auth, AuthConfig, RoleHierarchy, SessionData,
};
use std::time::Instant;

fn bench<F: FnMut() -> u64>(name: &str, mut f: F, iters: u64) {
    for _ in 0..3 {
        let _ = f();
    }
    let start = Instant::now();
    let n = f();
    let elapsed = start.elapsed();
    let total_ns = elapsed.as_nanos().max(1) as f64;
    let mean_ns = total_ns / iters as f64;
    let ops = (iters as f64) / (total_ns / 1e9);
    println!(
        "{name}: n={n} mean={mean_ns:.1} ns/op  {ops:.0} ops/sec  total={:.3} ms",
        total_ns / 1e6
    );
}

fn fast_auth() -> Auth {
    let mut cfg = AuthConfig::new(b"bench-secret-key-32-bytes-long!!!").unwrap();
    cfg.pass_ctx = context_from_opts(Some("bcrypt"), Some(4), None, None).unwrap();
    let mut h = RoleHierarchy::new();
    h.insert("admin".into(), vec!["editor".into(), "viewer".into()]);
    h.insert("editor".into(), vec!["viewer".into()]);
    cfg.hierarchy = h;
    Auth::with_config(cfg)
}

fn main() {
    let secret = b"bench-secret-key-32-bytes-long!!!";
    let auth = fast_auth();

    let mut session = SessionData::new("alice");
    session.roles = vec!["admin".into()];
    let token = sign_session(secret, &session).unwrap();

    bench(
        "session_sign x50k",
        || {
            for _ in 0..50_000 {
                let _ = sign_session(secret, &session).unwrap();
            }
            50_000
        },
        50_000,
    );

    bench(
        "session_load x50k",
        || {
            for _ in 0..50_000 {
                let _ = load_session(secret, &token, Some(86_400)).unwrap();
            }
            50_000
        },
        50_000,
    );

    let csrf = csrf_issue(secret, &session.session_id).unwrap();
    bench(
        "csrf_issue x100k",
        || {
            for _ in 0..100_000 {
                let _ = csrf_issue(secret, &session.session_id).unwrap();
            }
            100_000
        },
        100_000,
    );

    bench(
        "csrf_validate x200k",
        || {
            for _ in 0..200_000 {
                let _ = csrf_validate(secret, &session.session_id, &csrf);
            }
            200_000
        },
        200_000,
    );

    let roles = vec!["admin".to_string()];
    let hier = auth.config.hierarchy.clone();
    bench(
        "rbac_expand x500k",
        || {
            for _ in 0..500_000 {
                let _ = expand_roles(&hier, &roles);
            }
            500_000
        },
        500_000,
    );

    bench(
        "rbac_allows x500k",
        || {
            for _ in 0..500_000 {
                let _ = allows(&hier, &roles, "viewer");
            }
            500_000
        },
        500_000,
    );

    bench(
        "compare x1M",
        || {
            for _ in 0..1_000_000 {
                let _ = compare("csrf-token-value-abc", "csrf-token-value-abc");
            }
            1_000_000
        },
        1_000_000,
    );

    // Naive baseline: string equality (not constant-time)
    bench(
        "naive_eq x1M (baseline)",
        || {
            for _ in 0..1_000_000 {
                let _ = "csrf-token-value-abc" == "csrf-token-value-abc";
            }
            1_000_000
        },
        1_000_000,
    );

    bench(
        "generate_token32 x50k",
        || {
            for _ in 0..50_000 {
                let _ = generate_token(32).unwrap();
            }
            50_000
        },
        50_000,
    );

    // Password verify (bcrypt cost 4) — slower path
    let hash = auth.hash_password("bench-password").unwrap();
    bench(
        "bcrypt4_verify x200",
        || {
            for _ in 0..200 {
                let _ = auth.verify_password("bench-password", &hash).unwrap();
            }
            200
        },
        200,
    );

    // End-to-end login_user + sign (no password hash)
    bench(
        "login_user+sign x20k",
        || {
            for _ in 0..20_000 {
                let s = auth.login_user("alice", &roles, &[]).unwrap();
                let _ = auth.sign_session(&s).unwrap();
            }
            20_000
        },
        20_000,
    );
}
