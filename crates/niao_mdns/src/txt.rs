//! DNS-SD TXT record pack / unpack (RFC 6763).

use crate::error::{MdnsError, MdnsResult};
use std::collections::BTreeMap;

/// Pack a map of properties into TXT rdata (`key=value` length-prefixed strings).
/// Empty map yields a single empty string (RFC 6763 §6.1).
pub fn pack_txt(props: &BTreeMap<String, String>) -> MdnsResult<Vec<u8>> {
    if props.is_empty() {
        return Ok(vec![0]);
    }
    let mut out = Vec::new();
    for (k, v) in props {
        if k.is_empty() {
            return Err(MdnsError::Invalid("TXT key must not be empty".into()));
        }
        if k.as_bytes().iter().any(|&b| b == b'=') {
            return Err(MdnsError::Invalid(format!(
                "TXT key must not contain '=': '{k}'"
            )));
        }
        let entry = if v.is_empty() {
            k.clone()
        } else {
            format!("{k}={v}")
        };
        let bytes = entry.as_bytes();
        if bytes.len() > 255 {
            return Err(MdnsError::Encode(format!(
                "TXT entry longer than 255 bytes: '{k}'"
            )));
        }
        out.push(bytes.len() as u8);
        out.extend_from_slice(bytes);
    }
    Ok(out)
}

/// Unpack TXT rdata into a key→value map. Values without `=` are stored as empty string.
pub fn unpack_txt(data: &[u8]) -> MdnsResult<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    let mut i = 0usize;
    while i < data.len() {
        let len = data[i] as usize;
        i += 1;
        if i + len > data.len() {
            return Err(MdnsError::Decode("truncated TXT string".into()));
        }
        if len == 0 {
            // empty string; skip
            continue;
        }
        let slice = &data[i..i + len];
        i += len;
        let s = std::str::from_utf8(slice)
            .map_err(|_| MdnsError::Decode("non-UTF-8 TXT entry".into()))?;
        if let Some((k, v)) = s.split_once('=') {
            if k.is_empty() {
                return Err(MdnsError::Decode("empty TXT key".into()));
            }
            map.insert(k.to_string(), v.to_string());
        } else {
            map.insert(s.to_string(), String::new());
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_props() {
        let mut m = BTreeMap::new();
        m.insert("path".into(), "/".into());
        m.insert("version".into(), "1.0".into());
        m.insert("flag".into(), String::new());
        let packed = pack_txt(&m).unwrap();
        let back = unpack_txt(&packed).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn empty_map() {
        let packed = pack_txt(&BTreeMap::new()).unwrap();
        assert_eq!(packed, vec![0]);
        let back = unpack_txt(&packed).unwrap();
        assert!(back.is_empty());
    }

    #[test]
    fn reject_long_entry() {
        let mut m = BTreeMap::new();
        m.insert("k".into(), "x".repeat(300));
        assert!(pack_txt(&m).is_err());
    }

    #[test]
    fn unicode_value() {
        let mut m = BTreeMap::new();
        m.insert("name".into(), "猫".into());
        let packed = pack_txt(&m).unwrap();
        let back = unpack_txt(&packed).unwrap();
        assert_eq!(back.get("name").map(|s| s.as_str()), Some("猫"));
    }
}
