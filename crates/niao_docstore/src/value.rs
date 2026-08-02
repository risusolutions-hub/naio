//! Scalar index keys and dotted-path helpers for JSON documents.

use serde_json::{Map, Number, Value};
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// Key used in secondary indexes — only scalars are indexable.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IndexKey {
    Null,
    Bool(bool),
    Int(i64),
    /// Canonical f64 bits (NaN collapsed to one sentinel).
    Float(u64),
    Str(String),
}

impl IndexKey {
    pub fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::Null => Some(IndexKey::Null),
            Value::Bool(b) => Some(IndexKey::Bool(*b)),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Some(IndexKey::Int(i))
                } else if let Some(u) = n.as_u64() {
                    if u <= i64::MAX as u64 {
                        Some(IndexKey::Int(u as i64))
                    } else {
                        n.as_f64().map(|f| IndexKey::Float(canon_f64_bits(f)))
                    }
                } else {
                    n.as_f64().map(|f| IndexKey::Float(canon_f64_bits(f)))
                }
            }
            Value::String(s) => Some(IndexKey::Str(s.clone())),
            Value::Array(_) | Value::Object(_) => None,
        }
    }
}

fn canon_f64_bits(f: f64) -> u64 {
    if f.is_nan() {
        f64::NAN.to_bits()
    } else {
        f.to_bits()
    }
}

/// Resolve a dotted path like `"address.city"` on a JSON value.
pub fn get_path<'a>(doc: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(doc);
    }
    let mut cur = doc;
    for part in path.split('.') {
        match cur {
            Value::Object(map) => cur = map.get(part)?,
            _ => return None,
        }
    }
    Some(cur)
}

/// Set a dotted path, creating intermediate objects as needed.
pub fn set_path(doc: &mut Value, path: &str, value: Value) -> Result<(), String> {
    if path.is_empty() {
        *doc = value;
        return Ok(());
    }
    let parts: Vec<&str> = path.split('.').collect();
    if !doc.is_object() {
        *doc = Value::Object(Map::new());
    }
    let mut cur = doc;
    for (i, part) in parts.iter().enumerate() {
        if i + 1 == parts.len() {
            let obj = cur
                .as_object_mut()
                .ok_or_else(|| format!("cannot set field '{part}' on non-object"))?;
            obj.insert((*part).to_string(), value);
            return Ok(());
        }
        let obj = cur
            .as_object_mut()
            .ok_or_else(|| format!("cannot traverse '{part}' on non-object"))?;
        let needs = !obj.get(*part).map(|v| v.is_object()).unwrap_or(false);
        if needs {
            obj.insert((*part).to_string(), Value::Object(Map::new()));
        }
        cur = obj.get_mut(*part).unwrap();
    }
    Ok(())
}

/// Deep-merge `patch` fields into `doc` (objects merge; other types replace).
pub fn merge_patch(doc: &mut Value, patch: &Value) {
    match (doc, patch) {
        (Value::Object(dst), Value::Object(src)) => {
            for (k, v) in src {
                if v.is_null() {
                    dst.remove(k);
                } else if let Some(existing) = dst.get_mut(k) {
                    merge_patch(existing, v);
                } else {
                    dst.insert(k.clone(), v.clone());
                }
            }
        }
        (dst, src) => {
            *dst = src.clone();
        }
    }
}

/// Compare two JSON values with TinyDB-ish ordering (numbers cross int/float).
pub fn cmp_values(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Null, Value::Null) => Some(Ordering::Equal),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        (Value::Number(x), Value::Number(y)) => cmp_numbers(x, y),
        _ => None,
    }
}

fn cmp_numbers(a: &Number, b: &Number) -> Option<Ordering> {
    if let (Some(ai), Some(bi)) = (a.as_i64(), b.as_i64()) {
        return Some(ai.cmp(&bi));
    }
    let af = a.as_f64()?;
    let bf = b.as_f64()?;
    af.partial_cmp(&bf)
}

pub fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => cmp_numbers(x, y) == Some(Ordering::Equal),
        _ => a == b,
    }
}

/// Strip reserved `_id` from a user-supplied document.
pub fn strip_id(mut doc: Value) -> Value {
    if let Value::Object(ref mut map) = doc {
        map.remove("_id");
    }
    doc
}

/// Attach `_id` for return to callers.
pub fn with_id(mut doc: Value, id: u64) -> Value {
    if let Value::Object(ref mut map) = doc {
        map.insert("_id".to_string(), Value::Number(id.into()));
    } else {
        let mut map = Map::new();
        map.insert("_id".to_string(), Value::Number(id.into()));
        map.insert("value".to_string(), doc);
        return Value::Object(map);
    }
    doc
}

/// Serialize documents map to TinyDB-style table object (`{"1": {...}, ...}`).
pub fn table_to_json(docs: &BTreeMap<u64, Value>) -> Value {
    let mut map = Map::new();
    for (id, doc) in docs {
        map.insert(id.to_string(), doc.clone());
    }
    Value::Object(map)
}

/// Parse TinyDB-style table object into doc map.
pub fn table_from_json(v: &Value) -> Result<BTreeMap<u64, Value>, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "table must be a JSON object".to_string())?;
    let mut docs = BTreeMap::new();
    for (k, doc) in obj {
        let id: u64 = k
            .parse()
            .map_err(|_| format!("invalid document id key '{k}'"))?;
        let mut clean = doc.clone();
        if let Value::Object(ref mut m) = clean {
            m.remove("_id");
        }
        docs.insert(id, clean);
    }
    Ok(docs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dotted_path() {
        let doc = json!({"address": {"city": "Kyoto"}});
        assert_eq!(get_path(&doc, "address.city"), Some(&json!("Kyoto")));
        assert!(get_path(&doc, "address.zip").is_none());
    }

    #[test]
    fn merge_nested() {
        let mut doc = json!({"a": 1, "b": {"c": 2}});
        merge_patch(&mut doc, &json!({"b": {"d": 3}, "e": 4}));
        assert_eq!(doc, json!({"a": 1, "b": {"c": 2, "d": 3}, "e": 4}));
    }
}
