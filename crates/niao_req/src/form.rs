//! application/x-www-form-urlencoded encode/decode.

use crate::error::{ReqError, ReqResult};
use niao_http::{form_urlencode, percent_decode};
use std::collections::BTreeMap;

/// Encode key/value pairs as `application/x-www-form-urlencoded`.
pub fn encode_form(pairs: &[(String, String)]) -> String {
    let mut out = String::new();
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push('&');
        }
        out.push_str(&form_urlencode(k.as_bytes()));
        out.push('=');
        out.push_str(&form_urlencode(v.as_bytes()));
    }
    out
}

/// Encode a map (sorted keys for stable output).
pub fn encode_form_map(map: &BTreeMap<String, String>) -> String {
    let pairs: Vec<(String, String)> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    encode_form(&pairs)
}

/// Decode a form body into ordered pairs (preserves duplicate keys).
pub fn decode_form(body: &str) -> ReqResult<Vec<(String, String)>> {
    if body.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for part in body.split('&') {
        if part.is_empty() {
            continue;
        }
        let (k, v) = match part.split_once('=') {
            Some((a, b)) => (a, b),
            None => (part, ""),
        };
        let key = percent_decode(k).map_err(ReqError::Url)?;
        let val = percent_decode(v).map_err(ReqError::Url)?;
        out.push((key, val));
    }
    Ok(out)
}

/// Decode into a map (last value wins on duplicate keys).
pub fn decode_form_map(body: &str) -> ReqResult<BTreeMap<String, String>> {
    let mut m = BTreeMap::new();
    for (k, v) in decode_form(body)? {
        m.insert(k, v);
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let pairs = vec![
            ("hello".into(), "world".into()),
            ("a b".into(), "c&d".into()),
            ("空".into(), "値".into()),
        ];
        let enc = encode_form(&pairs);
        let dec = decode_form(&enc).unwrap();
        assert_eq!(dec, pairs);
    }

    #[test]
    fn empty() {
        assert_eq!(encode_form(&[]), "");
        assert!(decode_form("").unwrap().is_empty());
    }
}
