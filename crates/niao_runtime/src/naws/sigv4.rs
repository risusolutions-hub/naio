//! AWS Signature Version 4 — pure std implementation.
//!
//! Uses `niao_crypto::{sha256, hmac_sha256, hex}` exclusively (zero new crates).

use niao_crypto::{hex, hmac_sha256, sha256};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

// ── date/time helpers ────────────────────────────────────────────────────────

/// Current UTC timestamp as (amz_datetime `YYYYMMDDTHHMMSSZ`, date `YYYYMMDD`).
pub fn now_amz() -> (String, String) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    secs_to_amz(secs)
}

/// Convert Unix epoch seconds → (`YYYYMMDDTHHMMSSZ`, `YYYYMMDD`). Exposed for tests.
pub fn secs_to_amz(secs: u64) -> (String, String) {
    let (y, mo, d) = civil_from_days(secs / 86400);
    let rem = secs % 86400;
    let h = rem / 3600;
    let mi = (rem % 3600) / 60;
    let s = rem % 60;
    let dt = format!("{y:04}{mo:02}{d:02}T{h:02}{mi:02}{s:02}Z");
    let date = format!("{y:04}{mo:02}{d:02}");
    (dt, date)
}

/// Howard Hinnant civil_from_days: days-since-Unix-epoch → (year, month, day).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days as i64 + 719_468;
    let era: i64 = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // day of era [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // year of era [0, 399]
    let y = yoe + (era as u64) * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // month prime [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // day [1, 31]
    let mo = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let y = if mo <= 2 { y + 1 } else { y };
    (y, mo, d)
}

// ── percent-encoding ─────────────────────────────────────────────────────────

const HEX_UP: &[u8] = b"0123456789ABCDEF";

/// AWS-spec URI encoding: encode everything except unreserved chars.
/// `encode_slash = true` → encode `/` as `%2F` (for query keys/values).
/// `encode_slash = false` → keep `/` as-is (for path segments).
pub fn uri_encode(s: &str, encode_slash: bool) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b'/' if !encode_slash => out.push('/'),
            _ => {
                out.push('%');
                out.push(HEX_UP[(b >> 4) as usize] as char);
                out.push(HEX_UP[(b & 0xf) as usize] as char);
            }
        }
    }
    out
}

/// Build canonical query string: percent-encode then sort by key, then value.
pub fn canonical_query_str(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let mut pairs: Vec<(String, String)> = raw
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|kv| {
            let mut it = kv.splitn(2, '=');
            let k = it.next().unwrap_or("");
            let v = it.next().unwrap_or("");
            (uri_encode(k, true), uri_encode(v, true))
        })
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

// ── signing ──────────────────────────────────────────────────────────────────

/// All inputs required to produce a SigV4-signed Authorization header.
pub struct SignInput<'a> {
    pub method: &'a str,
    pub host: &'a str,
    pub path: &'a str,   // URI path (forward-slashes already present)
    pub query: &'a str,  // raw query string, without leading '?'
    pub region: &'a str,
    pub service: &'a str,
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub session_token: Option<&'a str>,
    pub body: &'a [u8],
    pub amz_datetime: &'a str, // "YYYYMMDDTHHMMSSZ"
    pub amz_date: &'a str,     // "YYYYMMDD"
    /// Extra headers to sign (lowercase name). Do NOT include host/x-amz-date.
    pub extra_headers: &'a [(&'a str, &'a str)],
}

/// Output: signed headers to attach to the HTTP request.
pub struct Signed {
    pub headers: Vec<(String, String)>,
}

/// Produce SigV4 signature and return all required request headers.
pub fn sign(inp: &SignInput<'_>) -> Signed {
    let payload_hash = hex::encode(&sha256(inp.body));

    // Collect headers in a sorted map (BTreeMap → lexicographic order).
    let mut hmap: BTreeMap<String, String> = BTreeMap::new();
    hmap.insert("host".into(), inp.host.into());
    hmap.insert("x-amz-content-sha256".into(), payload_hash.clone());
    hmap.insert("x-amz-date".into(), inp.amz_datetime.into());
    if let Some(tok) = inp.session_token {
        hmap.insert("x-amz-security-token".into(), tok.into());
    }
    for &(k, v) in inp.extra_headers {
        hmap.insert(k.to_lowercase(), v.into());
    }

    let canonical_headers: String = hmap.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed_headers: String = hmap.keys().cloned().collect::<Vec<_>>().join(";");
    let cqs = canonical_query_str(inp.query);

    // Task 1 — canonical request.
    let canonical_request = format!(
        "{method}\n{path}\n{query}\n{headers}\n{signed}\n{phash}",
        method = inp.method.to_uppercase(),
        path = uri_encode(inp.path, false),
        query = cqs,
        headers = canonical_headers,
        signed = signed_headers,
        phash = payload_hash,
    );

    let credential_scope = format!(
        "{}/{}/{}/aws4_request",
        inp.amz_date, inp.region, inp.service
    );

    // Task 2 — string to sign.
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        inp.amz_datetime,
        credential_scope,
        hex::encode(&sha256(canonical_request.as_bytes()))
    );

    // Task 3 — derive signing key.
    let k_date = hmac_sha256(
        format!("AWS4{}", inp.secret_key).as_bytes(),
        inp.amz_date.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, inp.region.as_bytes());
    let k_service = hmac_sha256(&k_region, inp.service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex::encode(&hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    // Task 4 — Authorization header.
    let auth = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        inp.access_key, credential_scope, signed_headers, signature
    );

    let mut headers: Vec<(String, String)> = hmap.into_iter().collect();
    headers.push(("authorization".into(), auth));
    Signed { headers }
}

// ── unit tests (AWS SigV4 test suite) ────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Known epoch seconds for 20150830T123600Z (AWS SigV4 test suite).
    const TEST_SECS: u64 = 1_440_938_160;

    #[test]
    fn date_roundtrip_2015_08_30() {
        let (dt, d) = secs_to_amz(TEST_SECS);
        assert_eq!(dt, "20150830T123600Z");
        assert_eq!(d, "20150830");
    }

    #[test]
    fn date_epoch_zero() {
        let (dt, d) = secs_to_amz(0);
        assert_eq!(dt, "19700101T000000Z");
        assert_eq!(d, "19700101");
    }

    #[test]
    fn date_leap_day() {
        // 2000-02-29 00:00:00 UTC = 951782400
        let (dt, d) = secs_to_amz(951_782_400);
        assert_eq!(dt, "20000229T000000Z");
        assert_eq!(d, "20000229");
    }

    #[test]
    fn uri_encode_unreserved() {
        assert_eq!(uri_encode("abc-_.~", true), "abc-_.~");
    }

    #[test]
    fn uri_encode_space() {
        assert_eq!(uri_encode("hello world", true), "hello%20world");
    }

    #[test]
    fn uri_encode_slash_true() {
        assert_eq!(uri_encode("a/b", true), "a%2Fb");
    }

    #[test]
    fn uri_encode_slash_false() {
        assert_eq!(uri_encode("a/b", false), "a/b");
    }

    #[test]
    fn canonical_query_sorted() {
        let q = canonical_query_str("Version=2010-05-08&Action=ListUsers");
        assert_eq!(q, "Action=ListUsers&Version=2010-05-08");
    }

    /// AWS SigV4 test suite — GET request with query parameters, empty body.
    /// Reference: https://docs.aws.amazon.com/general/latest/gr/sigv4-create-canonical-request.html
    #[test]
    fn sigv4_aws_get_test_vector() {
        // Inputs from AWS test suite "aws4_testsuite/get-vanilla-query-order-keys"
        let (amz_dt, amz_d) = secs_to_amz(TEST_SECS);
        let inp = SignInput {
            method: "GET",
            host: "example.amazonaws.com",
            path: "/",
            query: "Param1=value1&Param2=value2",
            region: "us-east-1",
            service: "service",
            access_key: "AKIDEXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            session_token: None,
            body: b"",
            amz_datetime: &amz_dt,
            amz_date: &amz_d,
            extra_headers: &[],
        };
        let signed = sign(&inp);

        // Authorization header must be present and start correctly.
        let auth = signed
            .headers
            .iter()
            .find(|(k, _)| k == "authorization")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();

        assert!(auth.starts_with("AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request"));
        assert!(auth.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date"));
        assert!(auth.contains("Signature="));
    }

    /// Signing key derivation test vector from AWS docs.
    #[test]
    fn signing_key_derivation() {
        // From AWS signing key derivation example
        let secret = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
        let date = "20150830";
        let region = "us-east-1";
        let service = "iam";

        let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
        let k_region = hmac_sha256(&k_date, region.as_bytes());
        let k_service = hmac_sha256(&k_region, service.as_bytes());
        let k_signing = hmac_sha256(&k_service, b"aws4_request");

        // kSigning for this derivation chain (verified against niao_crypto HMAC-SHA256).
        let expected = "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9";
        assert_eq!(hex::encode(&k_signing), expected);
    }
}
