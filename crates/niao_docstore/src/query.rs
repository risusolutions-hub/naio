//! Query matching for document predicates (~TinyDB Query).

use crate::value::{cmp_values, get_path, values_equal};
use serde_json::Value;
use std::cmp::Ordering;

/// Errors from malformed query objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    Invalid(String),
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryError::Invalid(m) => write!(f, "{m}"),
        }
    }
}

/// Test whether `doc` matches `query`.
///
/// Query shapes:
/// - `{field: value, ...}` — AND of equality on each field (shorthand)
/// - `{eq: {field: value}}` / `{ne|gt|gte|lt|lte|in|nin|contains: {field: value}}`
/// - `{exists: "field"}` or `{exists: {field: true|false}}`
/// - `{and: [q1, q2, ...]}` / `{or: [...]}` / `{not: q}`
pub fn matches(doc: &Value, query: &Value) -> Result<bool, QueryError> {
    let obj = query
        .as_object()
        .ok_or_else(|| QueryError::Invalid("query must be an object".into()))?;

    if obj.is_empty() {
        return Ok(true);
    }

    // Operator forms take precedence when a single known op key is present.
    if obj.len() == 1 {
        let (op, arg) = obj.iter().next().unwrap();
        match op.as_str() {
            "and" => return match_and(doc, arg),
            "or" => return match_or(doc, arg),
            "not" => return Ok(!matches(doc, arg)?),
            "eq" => return match_cmp(doc, arg, |o| o == Ordering::Equal),
            "ne" => return match_cmp(doc, arg, |o| o != Ordering::Equal),
            "gt" => return match_cmp(doc, arg, |o| o == Ordering::Greater),
            "gte" => {
                return match_cmp(doc, arg, |o| o == Ordering::Greater || o == Ordering::Equal)
            }
            "lt" => return match_cmp(doc, arg, |o| o == Ordering::Less),
            "lte" => return match_cmp(doc, arg, |o| o == Ordering::Less || o == Ordering::Equal),
            "in" => return match_in(doc, arg, true),
            "nin" => return match_in(doc, arg, false),
            "contains" => return match_contains(doc, arg),
            "exists" => return match_exists(doc, arg),
            _ => {}
        }
    }

    // Multi-key object or unknown single key → field equality AND.
    // If any key is a reserved op mixed with fields, reject.
    for key in obj.keys() {
        if is_op(key) {
            return Err(QueryError::Invalid(format!(
                "cannot mix operator '{key}' with field equalities in one query object"
            )));
        }
    }
    for (field, expected) in obj {
        let actual = get_path(doc, field);
        match actual {
            Some(v) if values_equal(v, expected) => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

fn is_op(key: &str) -> bool {
    matches!(
        key,
        "and"
            | "or"
            | "not"
            | "eq"
            | "ne"
            | "gt"
            | "gte"
            | "lt"
            | "lte"
            | "in"
            | "nin"
            | "contains"
            | "exists"
    )
}

fn match_and(doc: &Value, arg: &Value) -> Result<bool, QueryError> {
    let arr = arg
        .as_array()
        .ok_or_else(|| QueryError::Invalid("'and' expects an array of queries".into()))?;
    for q in arr {
        if !matches(doc, q)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn match_or(doc: &Value, arg: &Value) -> Result<bool, QueryError> {
    let arr = arg
        .as_array()
        .ok_or_else(|| QueryError::Invalid("'or' expects an array of queries".into()))?;
    if arr.is_empty() {
        return Ok(false);
    }
    for q in arr {
        if matches(doc, q)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn single_field(arg: &Value) -> Result<(&str, &Value), QueryError> {
    let obj = arg
        .as_object()
        .ok_or_else(|| QueryError::Invalid("comparison operand must be {field: value}".into()))?;
    if obj.len() != 1 {
        return Err(QueryError::Invalid(
            "comparison operand must have exactly one field".into(),
        ));
    }
    let (k, v) = obj.iter().next().unwrap();
    Ok((k.as_str(), v))
}

fn match_cmp(
    doc: &Value,
    arg: &Value,
    pred: impl Fn(Ordering) -> bool,
) -> Result<bool, QueryError> {
    let (field, expected) = single_field(arg)?;
    let Some(actual) = get_path(doc, field) else {
        return Ok(false);
    };
    match cmp_values(actual, expected) {
        Some(ord) => Ok(pred(ord)),
        None => Ok(false),
    }
}

fn match_in(doc: &Value, arg: &Value, positive: bool) -> Result<bool, QueryError> {
    let (field, list_v) = single_field(arg)?;
    let list = list_v
        .as_array()
        .ok_or_else(|| QueryError::Invalid("'in'/'nin' value must be an array".into()))?;
    let Some(actual) = get_path(doc, field) else {
        return Ok(!positive);
    };
    let found = list.iter().any(|x| values_equal(actual, x));
    Ok(if positive { found } else { !found })
}

fn match_contains(doc: &Value, arg: &Value) -> Result<bool, QueryError> {
    let (field, needle) = single_field(arg)?;
    let Some(actual) = get_path(doc, field) else {
        return Ok(false);
    };
    match (actual, needle) {
        (Value::String(hay), Value::String(n)) => Ok(hay.contains(n.as_str())),
        (Value::Array(items), n) => Ok(items.iter().any(|x| values_equal(x, n))),
        _ => Ok(false),
    }
}

fn match_exists(doc: &Value, arg: &Value) -> Result<bool, QueryError> {
    match arg {
        Value::String(field) => Ok(get_path(doc, field).is_some()),
        Value::Object(map) if map.len() == 1 => {
            let (field, flag) = map.iter().next().unwrap();
            let want = flag
                .as_bool()
                .ok_or_else(|| QueryError::Invalid("'exists' flag must be a bool".into()))?;
            let present = get_path(doc, field).is_some();
            Ok(present == want)
        }
        _ => Err(QueryError::Invalid(
            "'exists' expects a field string or {field: bool}".into(),
        )),
    }
}

/// Extract a simple equality `(field, value)` from a query for index use, if any.
pub fn extract_eq_field(query: &Value) -> Option<(String, Value)> {
    let obj = query.as_object()?;
    if obj.len() == 1 {
        let (k, v) = obj.iter().next().unwrap();
        if k == "eq" {
            let inner = v.as_object()?;
            if inner.len() == 1 {
                let (f, val) = inner.iter().next().unwrap();
                return Some((f.clone(), val.clone()));
            }
        } else if !is_op(k) {
            return Some((k.clone(), v.clone()));
        }
        return None;
    }
    // Multi-field shorthand: prefer first indexed candidate (caller checks index).
    if obj.keys().any(|k| is_op(k)) {
        return None;
    }
    let (f, v) = obj.iter().next()?;
    Some((f.clone(), v.clone()))
}

/// Extract all equality fields from an AND / shorthand for multi-index planning.
pub fn extract_eq_fields(query: &Value) -> Vec<(String, Value)> {
    let Some(obj) = query.as_object() else {
        return Vec::new();
    };
    if obj.len() == 1 {
        let (k, v) = obj.iter().next().unwrap();
        if k == "and" {
            let mut out = Vec::new();
            if let Some(arr) = v.as_array() {
                for q in arr {
                    out.extend(extract_eq_fields(q));
                }
            }
            return out;
        }
        if k == "eq" {
            if let Some((f, val)) = extract_eq_field(query) {
                return vec![(f, val)];
            }
        }
        if !is_op(k) {
            return vec![(k.clone(), v.clone())];
        }
        return Vec::new();
    }
    if obj.keys().any(|k| is_op(k)) {
        return Vec::new();
    }
    obj.iter().map(|(f, v)| (f.clone(), v.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shorthand_and_ops() {
        let doc = json!({"name": "Ada", "age": 36});
        assert!(matches(&doc, &json!({"name": "Ada"})).unwrap());
        assert!(!matches(&doc, &json!({"name": "Bob"})).unwrap());
        assert!(matches(&doc, &json!({"gt": {"age": 30}})).unwrap());
        assert!(matches(
            &doc,
            &json!({"and": [{"name": "Ada"}, {"lt": {"age": 40}}]})
        )
        .unwrap());
        assert!(matches(&doc, &json!({"contains": {"name": "d"}})).unwrap());
        assert!(matches(&doc, &json!({"exists": "age"})).unwrap());
        assert!(!matches(&doc, &json!({"exists": "missing"})).unwrap());
    }
}
