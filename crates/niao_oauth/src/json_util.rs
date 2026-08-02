//! Shared JSON object helpers for niao_json_core `Object` type.

use niao_json_core::{Object, Value};
use std::collections::HashMap;

pub fn object_get_str(map: &Object, key: &str) -> Option<String> {
    map.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

pub fn object_require_str(
    map: &Object,
    key: &str,
    err: impl Fn(String) -> crate::error::OAuthError,
) -> Result<String, crate::error::OAuthError> {
    object_get_str(map, key).ok_or_else(|| err(format!("missing {key}")))
}

pub fn object_str_array(map: &Object, key: &str) -> Vec<String> {
    match map.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

pub fn object_as_map(map: &Object) -> HashMap<String, Value> {
    map.iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

pub fn value_to_object(v: Value) -> Result<Object, crate::error::OAuthError> {
    match v {
        Value::Object(o) => Ok(o),
        _ => Err(crate::error::OAuthError::Parse(
            "expected JSON object".into(),
        )),
    }
}

pub fn value_as_u64(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}
