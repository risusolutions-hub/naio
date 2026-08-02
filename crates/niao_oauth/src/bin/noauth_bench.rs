//! Micro-benchmark for noauth hot paths.
use niao_oauth::{
    auth_url, parse_callback_url, parse_token_json, pkce_challenge, pkce_pair, random_state,
    AuthUrlOptions, OAuthClient, PkceChallengeMethod,
};
use std::time::Instant;

fn sample_client() -> OAuthClient {
    OAuthClient::builder("bench-client", "https://idp.example.com/oauth/token")
        .authorization_endpoint("https://idp.example.com/oauth/authorize")
        .redirect_uri("https://app.example.com/callback")
        .scopes(vec!["openid".into(), "profile".into()])
        .build()
        .unwrap()
}

fn main() {
    let client = sample_client();
    let pkce = pkce_pair(true);
    let opts = AuthUrlOptions {
        state: Some(random_state()),
        code_challenge: Some(pkce.challenge.clone()),
        code_challenge_method: Some(PkceChallengeMethod::S256),
        ..Default::default()
    };

    let n = 500_000usize;

    let t0 = Instant::now();
    for _ in 0..n {
        let _ = auth_url(&client, &opts).unwrap();
    }
    let e0 = t0.elapsed();
    println!(
        "auth_url build: {n} runs in {:.2} ms ({:.0} ns/op)",
        e0.as_secs_f64() * 1000.0,
        e0.as_nanos() as f64 / n as f64
    );

    let t1 = Instant::now();
    for _ in 0..n {
        let _ = pkce_pair(true);
    }
    let e1 = t1.elapsed();
    println!(
        "pkce_pair (S256): {n} runs in {:.2} ms ({:.0} ns/op)",
        e1.as_secs_f64() * 1000.0,
        e1.as_nanos() as f64 / n as f64
    );

    let verifier = pkce.verifier.clone();
    let t2 = Instant::now();
    for _ in 0..n {
        let _ = pkce_challenge(&verifier, PkceChallengeMethod::S256);
    }
    let e2 = t2.elapsed();
    println!(
        "pkce_challenge S256: {n} runs in {:.2} ms ({:.0} ns/op)",
        e2.as_secs_f64() * 1000.0,
        e2.as_nanos() as f64 / n as f64
    );

    let cb = "https://app.example.com/callback?code=abc123&state=xyz789";
    let t3 = Instant::now();
    for _ in 0..n {
        let _ = parse_callback_url(cb).unwrap();
    }
    let e3 = t3.elapsed();
    println!(
        "parse_callback_url: {n} runs in {:.2} ms ({:.0} ns/op)",
        e3.as_secs_f64() * 1000.0,
        e3.as_nanos() as f64 / n as f64
    );

    let token_json = r#"{"access_token":"eyJhbG","token_type":"Bearer","expires_in":3600,"refresh_token":"rt_abc","scope":"openid profile","id_token":"eyJ.id.sig"}"#;
    let t4 = Instant::now();
    for _ in 0..n {
        let _ = parse_token_json(token_json).unwrap();
    }
    let e4 = t4.elapsed();
    println!(
        "parse_token_json: {n} runs in {:.2} ms ({:.0} ns/op)",
        e4.as_secs_f64() * 1000.0,
        e4.as_nanos() as f64 / n as f64
    );
}
