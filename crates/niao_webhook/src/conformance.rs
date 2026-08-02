//! Standard Webhooks conformance vectors (from official JS/Python SDK tests).

use crate::{make_headers, sign_request, VerifyOptions, Webhook, WebhookError, WebhookOptions};

const SECRET_B64: &str = "MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
const MSG_ID: &str = "msg_p5jXN8AQM9LWM0D4loKWxJek";
const TS: i64 = 1_614_265_330;
const PAYLOAD: &str = r#"{"test": 2432232314}"#;
const EXPECTED: &str = "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE=";

fn wh() -> Webhook {
    Webhook::new(&format!("whsec_{SECRET_B64}"), WebhookOptions::default()).unwrap()
}

fn opts(now: i64) -> VerifyOptions {
    VerifyOptions {
        now: Some(now),
        ..Default::default()
    }
}

#[test]
fn conformance_official_sign_vector() {
    assert_eq!(wh().sign(MSG_ID, TS, PAYLOAD).unwrap(), EXPECTED);
}

#[test]
fn conformance_verify_with_and_without_prefix() {
    let w1 = Webhook::new(SECRET_B64, WebhookOptions::default()).unwrap();
    let w2 = Webhook::new(&format!("whsec_{SECRET_B64}"), WebhookOptions::default()).unwrap();
    let sig = w1.sign(MSG_ID, TS, PAYLOAD).unwrap();
    let headers = make_headers(MSG_ID, TS, &sig);
    w1.verify(PAYLOAD, &headers, &opts(TS)).unwrap();
    w2.verify(PAYLOAD, &headers, &opts(TS)).unwrap();
}

#[test]
fn conformance_missing_id() {
    let mut headers = make_headers(MSG_ID, TS, &EXPECTED);
    headers.remove("webhook-id");
    assert!(matches!(
        wh().verify(PAYLOAD, &headers, &opts(TS)),
        Err(WebhookError::MissingHeaders)
    ));
}

#[test]
fn conformance_missing_timestamp() {
    let mut headers = make_headers(MSG_ID, TS, &EXPECTED);
    headers.remove("webhook-timestamp");
    assert!(matches!(
        wh().verify(PAYLOAD, &headers, &opts(TS)),
        Err(WebhookError::MissingHeaders)
    ));
}

#[test]
fn conformance_missing_signature() {
    let mut headers = make_headers(MSG_ID, TS, &EXPECTED);
    headers.remove("webhook-signature");
    assert!(matches!(
        wh().verify(PAYLOAD, &headers, &opts(TS)),
        Err(WebhookError::MissingHeaders)
    ));
}

#[test]
fn conformance_invalid_timestamp() {
    let sig = wh().sign(MSG_ID, TS, PAYLOAD).unwrap();
    let mut headers = make_headers(MSG_ID, TS, &sig);
    headers.insert("webhook-timestamp".into(), "hello".into());
    assert!(matches!(
        wh().verify(PAYLOAD, &headers, &opts(TS)),
        Err(WebhookError::InvalidTimestamp)
    ));
}

#[test]
fn conformance_invalid_signature() {
    let headers = make_headers(MSG_ID, TS, "v1,dawfeoifkpqwoekfpqoekf");
    assert!(matches!(
        wh().verify(PAYLOAD, &headers, &opts(TS)),
        Err(WebhookError::NoMatchingSignature)
    ));
}

#[test]
fn conformance_partial_signature() {
    let sig = wh().sign(MSG_ID, TS, PAYLOAD).unwrap();
    let partial = &sig[..8.min(sig.len())];
    let headers = make_headers(MSG_ID, TS, partial);
    assert!(matches!(
        wh().verify(PAYLOAD, &headers, &opts(TS)),
        Err(WebhookError::NoMatchingSignature)
    ));
    let headers = make_headers(MSG_ID, TS, "v1,");
    assert!(matches!(
        wh().verify(PAYLOAD, &headers, &opts(TS)),
        Err(WebhookError::NoMatchingSignature)
    ));
}

#[test]
fn conformance_multi_sig_header() {
    let good = wh().sign(MSG_ID, TS, PAYLOAD).unwrap();
    let combined = format!(
        "v1,Ceo5qEr07ixe2NLpvHk3FH9bwy/WavXrAFQ/9tdO6mc= v2,Ceo5qEr07ixe2NLpvHk3FH9bwy/WavXrAFQ/9tdO6mc= {good} v1,Ceo5qEr07ixe2NLpvHk3FH9bwy/WavXrAFQ/9tdO6mc="
    );
    let headers = make_headers(MSG_ID, TS, &combined);
    let v = wh().verify(PAYLOAD, &headers, &opts(TS)).unwrap();
    assert!(v.json.is_some());
}

#[test]
fn conformance_timestamp_too_old() {
    let old = TS - 301;
    let sig = wh().sign(MSG_ID, old, PAYLOAD).unwrap();
    let headers = make_headers(MSG_ID, old, &sig);
    assert!(matches!(
        wh().verify(
            PAYLOAD,
            &headers,
            &VerifyOptions {
                now: Some(TS),
                tolerance: 300,
                ..Default::default()
            }
        ),
        Err(WebhookError::TimestampTooOld)
    ));
}

#[test]
fn conformance_timestamp_too_new() {
    let future = TS + 301;
    let sig = wh().sign(MSG_ID, future, PAYLOAD).unwrap();
    let headers = make_headers(MSG_ID, future, &sig);
    assert!(matches!(
        wh().verify(
            PAYLOAD,
            &headers,
            &VerifyOptions {
                now: Some(TS),
                tolerance: 300,
                ..Default::default()
            }
        ),
        Err(WebhookError::TimestampTooNew)
    ));
}

#[test]
fn conformance_empty_payload() {
    let payload = "";
    let sig = wh().sign(MSG_ID, TS, payload).unwrap();
    let headers = make_headers(MSG_ID, TS, &sig);
    let v = wh().verify(payload, &headers, &opts(TS)).unwrap();
    assert!(v.json.is_none());
}

#[test]
fn conformance_sign_request() {
    let req = sign_request(&wh(), PAYLOAD, Some(MSG_ID), Some(TS)).unwrap();
    assert_eq!(req.id, MSG_ID);
    assert_eq!(req.timestamp, TS);
    assert_eq!(req.signature, EXPECTED);
    wh().verify(&req.payload, &req.headers, &opts(TS)).unwrap();
}

#[test]
fn conformance_unicode_payload() {
    let jp = "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}";
    let payload = format!(r#"{{"msg":"{jp}"}}"#);
    let sig = wh().sign(MSG_ID, TS, &payload).unwrap();
    let headers = make_headers(MSG_ID, TS, &sig);
    let v = wh().verify_raw(&payload, &headers, &opts(TS)).unwrap();
    assert_eq!(v.payload, payload);
    assert!(wh().valid(&payload, &headers, &opts(TS)));
}

#[test]
fn conformance_secret_rotation() {
    let old = Webhook::new(SECRET_B64, WebhookOptions::default()).unwrap();
    let new_secret = "AAAAAAAAAAAAAAAAAAAAAA==";
    let consumer =
        Webhook::with_secrets(&[new_secret, SECRET_B64], WebhookOptions::default().format).unwrap();
    let sig = old.sign(MSG_ID, TS, PAYLOAD).unwrap();
    let headers = make_headers(MSG_ID, TS, &sig);
    assert!(Webhook::new(new_secret, WebhookOptions::default())
        .unwrap()
        .verify(PAYLOAD, &headers, &opts(TS))
        .is_err());
    consumer.verify(PAYLOAD, &headers, &opts(TS)).unwrap();
}
