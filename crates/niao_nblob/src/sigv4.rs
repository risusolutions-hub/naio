//! AWS Signature Version 4 (subset used by S3) via `niao_crypto`.

use niao_crypto::{hex, hmac_sha256, sha256};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_amz() -> (String, String) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    secs_to_amz(secs)
}

pub fn secs_to_amz(secs: u64) -> (String, String) {
    let (y, mo, d) = civil_from_days(secs / 86400);
    let rem = secs % 86400;
    let h = rem / 3600;
    let mi = (rem % 3600) / 60;
    let s = rem % 60;
    (
        format!("{y:04}{mo:02}{d:02}T{h:02}{mi:02}{s:02}Z"),
        format!("{y:04}{mo:02}{d:02}"),
    )
}

fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days as i64 + 719_468;
    let era: i64 = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + (era as u64) * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    (y, mo, d)
}

const HEX_UP: &[u8] = b"0123456789ABCDEF";

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

fn canonical_query_str(raw: &str) -> String {
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

pub struct SignInput<'a> {
    pub method: &'a str,
    pub host: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    pub region: &'a str,
    pub service: &'a str,
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub session_token: Option<&'a str>,
    pub body: &'a [u8],
    pub amz_datetime: &'a str,
    pub amz_date: &'a str,
    pub extra_headers: &'a [(&'a str, &'a str)],
}

pub struct Signed {
    pub headers: Vec<(String, String)>,
}

pub fn sign(inp: &SignInput<'_>) -> Signed {
    let payload_hash = hex::encode(&sha256(inp.body));
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
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        inp.method.to_uppercase(),
        uri_encode(inp.path, false),
        cqs,
        canonical_headers,
        signed_headers,
        payload_hash,
    );
    let credential_scope = format!(
        "{}/{}/{}/aws4_request",
        inp.amz_date, inp.region, inp.service
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        inp.amz_datetime,
        credential_scope,
        hex::encode(&sha256(canonical_request.as_bytes()))
    );
    let k_date = hmac_sha256(
        format!("AWS4{}", inp.secret_key).as_bytes(),
        inp.amz_date.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, inp.region.as_bytes());
    let k_service = hmac_sha256(&k_region, inp.service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex::encode(&hmac_sha256(&k_signing, string_to_sign.as_bytes()));
    let auth = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        inp.access_key, credential_scope, signed_headers, signature
    );
    let mut headers: Vec<(String, String)> = hmap.into_iter().collect();
    headers.push(("authorization".into(), auth));
    Signed { headers }
}
