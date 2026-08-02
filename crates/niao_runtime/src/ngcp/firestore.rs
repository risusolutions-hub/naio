//! Firestore REST — get / set / delete / query.

use super::{
    bearer_auth, gcp_error, json_escape, ok_string, ok_value, with_config_mut, GcpResult,
};
use crate::{Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_json_core::{Object as JsonObject, Value as JsonValue};
use std::collections::HashMap;

fn fs_error(span: Span, msg: impl Into<String>) -> ValueRef {
    gcp_error(codes::E4541_NGCP_ERROR, "ngcp_firestore_error", msg, span)
}

fn doc_path(project: &str, collection: &str, doc_id: &str) -> String {
    format!(
        "https://firestore.googleapis.com/v1/projects/{}/databases/(default)/documents/{}/{}",
        crate::ngcp::auth::uri_encode_path(project),
        crate::ngcp::auth::uri_encode_path(collection),
        crate::ngcp::auth::uri_encode_path(doc_id)
    )
}

/// Encode a Niao value as a Firestore REST typed value JSON object.
pub fn encode_fs_value(v: &Value) -> String {
    match v {
        Value::Nil => "{\"nullValue\":null}".into(),
        Value::Bool(b) => format!("{{\"booleanValue\":{b}}}"),
        Value::Int(n) => format!("{{\"integerValue\":\"{n}\"}}"),
        Value::Float(f) => format!("{{\"doubleValue\":{f}}}"),
        Value::String(s) => format!("{{\"stringValue\":\"{}\"}}", json_escape(s)),
        Value::Array(arr) => {
            let parts: Vec<_> = arr
                .iter()
                .map(|x| encode_fs_value(&x.borrow()))
                .collect();
            format!("{{\"arrayValue\":{{\"values\":[{}]}}}}", parts.join(","))
        }
        Value::Object(map) => {
            let mut parts = Vec::with_capacity(map.len());
            for (k, val) in map {
                parts.push(format!(
                    "\"{}\":{}",
                    json_escape(k),
                    encode_fs_value(&val.borrow())
                ));
            }
            format!(
                "{{\"mapValue\":{{\"fields\":{{{}}}}}}}",
                parts.join(",")
            )
        }
        other => format!(
            "{{\"stringValue\":\"{}\"}}",
            json_escape(&other.to_string())
        ),
    }
}

/// Encode a Niao object as Firestore `fields` map.
pub fn encode_fields(map: &HashMap<String, ValueRef>) -> String {
    let mut parts = Vec::with_capacity(map.len());
    for (k, v) in map {
        parts.push(format!(
            "\"{}\":{}",
            json_escape(k),
            encode_fs_value(&v.borrow())
        ));
    }
    format!("{{{}}}", parts.join(","))
}

fn decode_fields_object(fields: &JsonObject) -> HashMap<String, ValueRef> {
    let mut map = HashMap::with_capacity(fields.len());
    for (k, v) in fields.iter() {
        map.insert(k.to_string(), decode_fs_value(v).ref_cell());
    }
    map
}

/// Decode Firestore typed value JSON into a Niao `Value`.
pub fn decode_fs_value(v: &JsonValue) -> Value {
    let Some(obj) = v.as_object() else {
        return Value::Nil;
    };
    if obj.get("nullValue").is_some() {
        return Value::Nil;
    }
    if let Some(b) = obj.get("booleanValue").and_then(|v| v.as_bool()) {
        return Value::Bool(b);
    }
    if let Some(s) = obj.get("integerValue").and_then(|v| v.as_str()) {
        if let Ok(n) = s.parse::<i64>() {
            return Value::Int(n);
        }
    }
    if let Some(n) = obj.get("doubleValue").and_then(|v| v.as_f64()) {
        return Value::Float(n);
    }
    if let Some(s) = obj.get("stringValue").and_then(|v| v.as_str()) {
        return Value::String(s.to_string());
    }
    if let Some(s) = obj.get("timestampValue").and_then(|v| v.as_str()) {
        return Value::String(s.to_string());
    }
    if let Some(s) = obj.get("referenceValue").and_then(|v| v.as_str()) {
        return Value::String(s.to_string());
    }
    if let Some(n) = obj.get("arrayValue") {
        let mut out = Vec::new();
        if let Some(items) = n
            .as_object()
            .and_then(|m| m.get("values"))
            .and_then(|v| v.as_array())
        {
            for item in items {
                out.push(decode_fs_value(item).ref_cell());
            }
        }
        return Value::Array(out);
    }
    if let Some(map_obj) = obj.get("mapValue").and_then(|v| v.as_object()) {
        let fields = map_obj
            .get("fields")
            .and_then(|v| v.as_object())
            .map(decode_fields_object)
            .unwrap_or_default();
        return Value::Object(fields);
    }
    Value::Nil
}

/// Decode a Firestore document `fields` object into a Niao map.
pub fn decode_fields_map(json: &str) -> HashMap<String, ValueRef> {
    let parsed = match niao_json_core::parse(json) {
        Ok(v) => v,
        Err(_) => return HashMap::new(),
    };
    let Some(root) = parsed.as_object() else {
        return HashMap::new();
    };
    if let Some(fields) = root.get("fields").and_then(|v| v.as_object()) {
        return decode_fields_object(fields);
    }
    if let Some(doc) = root.get("document").and_then(|v| v.as_object()) {
        if let Some(fields) = doc.get("fields").and_then(|v| v.as_object()) {
            return decode_fields_object(fields);
        }
    }
    let mut map = HashMap::new();
    if let Some(arr) = parsed.as_array() {
        for item in arr {
            if let Some(doc) = item.get("document").and_then(|v| v.as_object()) {
                if let Some(fields) = doc.get("fields").and_then(|v| v.as_object()) {
                    map = decode_fields_object(fields);
                    break;
                }
            }
        }
    }
    map
}

/// `ngcp.firestore_get(cfg, collection, doc_id) → fields{} | nil`
///
/// // >>> ngcp.firestore_get != nil
/// // => true
pub fn firestore_get(args: &[ValueRef], span: Span) -> GcpResult {
    if args.len() != 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E4540_NGCP_ARITY,
            "ngcp_firestore_get() expects 3 arguments: config, collection, doc_id",
        ));
    }
    let id = super::int_arg(args, 0, "ngcp_firestore_get", span)?;
    let collection = super::str_arg(args, 1, "ngcp_firestore_get", span)?;
    let doc_id = super::str_arg(args, 2, "ngcp_firestore_get", span)?;

    match with_config_mut(id, span, |cfg| {
        let token = match bearer_auth(cfg) {
            Ok(t) => t,
            Err(e) => return fs_error(span, e),
        };
        let url = doc_path(&cfg.project, &collection, &doc_id);
        match niao_http::get(&url)
            .set("Authorization", format!("Bearer {token}"))
            .send()
        {
            Ok(resp) => {
                let status = resp.status as i64;
                if status == 404 {
                    return Value::Nil.ref_cell();
                }
                if status >= 400 {
                    return fs_error(span, String::from_utf8_lossy(&resp.body));
                }
                let text = String::from_utf8_lossy(&resp.body);
                Value::Object(decode_fields_map(&text)).ref_cell()
            }
            Err(e) => fs_error(span, e.to_string()),
        }
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// `ngcp.firestore_set(cfg, collection, doc_id, fields{}) → true`
///
/// // >>> ngcp.firestore_set != nil
/// // => true
pub fn firestore_set(args: &[ValueRef], span: Span) -> GcpResult {
    if args.len() != 4 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E4540_NGCP_ARITY,
            "ngcp_firestore_set() expects 4 arguments: config, collection, doc_id, fields",
        ));
    }
    let id = super::int_arg(args, 0, "ngcp_firestore_set", span)?;
    let collection = super::str_arg(args, 1, "ngcp_firestore_set", span)?;
    let doc_id = super::str_arg(args, 2, "ngcp_firestore_set", span)?;
    let fields = super::obj_arg(args, 3, "ngcp_firestore_set", span)?;
    let fields_json = encode_fields(&fields);
    let body = format!("{{\"fields\":{fields_json}}}");

    match with_config_mut(id, span, |cfg| {
        let token = match bearer_auth(cfg) {
            Ok(t) => t,
            Err(e) => return fs_error(span, e),
        };
        let url = format!(
            "{}?{}",
            doc_path(&cfg.project, &collection, &doc_id),
            field_mask_query(&fields)
        );
        match niao_http::request(niao_http::Method::Patch, &url)
            .set("Authorization", format!("Bearer {token}"))
            .set("Content-Type", "application/json")
            .send_string(&body)
        {
            Ok(resp) => {
                if (resp.status as i64) >= 400 {
                    return fs_error(span, String::from_utf8_lossy(&resp.body));
                }
                Value::Bool(true).ref_cell()
            }
            Err(e) => fs_error(span, e.to_string()),
        }
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn field_mask_query(fields: &HashMap<String, ValueRef>) -> String {
    let mut parts = Vec::new();
    for k in fields.keys() {
        parts.push(format!(
            "updateMask.fieldPaths={}",
            crate::ngcp::auth::uri_encode_path(k)
        ));
    }
    if parts.is_empty() {
        String::new()
    } else {
        parts.join("&")
    }
}

/// `ngcp.firestore_delete(cfg, collection, doc_id) → true`
///
/// // >>> ngcp.firestore_delete != nil
/// // => true
pub fn firestore_delete(args: &[ValueRef], span: Span) -> GcpResult {
    if args.len() != 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E4540_NGCP_ARITY,
            "ngcp_firestore_delete() expects 3 arguments: config, collection, doc_id",
        ));
    }
    let id = super::int_arg(args, 0, "ngcp_firestore_delete", span)?;
    let collection = super::str_arg(args, 1, "ngcp_firestore_delete", span)?;
    let doc_id = super::str_arg(args, 2, "ngcp_firestore_delete", span)?;

    match with_config_mut(id, span, |cfg| {
        let token = match bearer_auth(cfg) {
            Ok(t) => t,
            Err(e) => return fs_error(span, e),
        };
        let url = doc_path(&cfg.project, &collection, &doc_id);
        match niao_http::delete(&url)
            .set("Authorization", format!("Bearer {token}"))
            .send()
        {
            Ok(resp) => {
                let status = resp.status as i64;
                if status >= 400 && status != 404 {
                    return fs_error(span, String::from_utf8_lossy(&resp.body));
                }
                Value::Bool(true).ref_cell()
            }
            Err(e) => fs_error(span, e.to_string()),
        }
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// `ngcp.firestore_query(cfg, collection, opts?) → docs[]`
///
/// `opts`: `{limit?: int}` — structured query over the collection.
///
/// // >>> ngcp.firestore_query != nil
/// // => true
pub fn firestore_query(args: &[ValueRef], span: Span) -> GcpResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E4540_NGCP_ARITY,
            "ngcp_firestore_query() expects 2-3 arguments: config, collection, opts?",
        ));
    }
    let id = super::int_arg(args, 0, "ngcp_firestore_query", span)?;
    let collection = super::str_arg(args, 1, "ngcp_firestore_query", span)?;
    let limit = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::Object(m) => m
                .get("limit")
                .and_then(|v| match &*v.borrow() {
                    Value::Int(n) => Some(*n),
                    _ => None,
                })
                .unwrap_or(100),
            Value::Int(n) => *n,
            _ => 100,
        }
    } else {
        100
    };

    match with_config_mut(id, span, |cfg| {
        let token = match bearer_auth(cfg) {
            Ok(t) => t,
            Err(e) => return fs_error(span, e),
        };
        let body = format!(
            "{{\"structuredQuery\":{{\"from\":[{{\"collectionId\":\"{}\"}}],\"limit\":{}}}}}",
            json_escape(&collection),
            limit
        );
        let url = format!(
            "https://firestore.googleapis.com/v1/projects/{}/databases/(default)/documents:runQuery",
            crate::ngcp::auth::uri_encode_path(&cfg.project)
        );
        match niao_http::post(&url)
            .set("Authorization", format!("Bearer {token}"))
            .set("Content-Type", "application/json")
            .send_string(&body)
        {
            Ok(resp) => {
                if (resp.status as i64) >= 400 {
                    return fs_error(span, String::from_utf8_lossy(&resp.body));
                }
                let text = String::from_utf8_lossy(&resp.body);
                Value::Array(parse_query_docs(&text)).ref_cell()
            }
            Err(e) => fs_error(span, e.to_string()),
        }
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn parse_query_docs(json: &str) -> Vec<ValueRef> {
    let parsed = match niao_json_core::parse(json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    let Some(items) = parsed.as_array() else {
        return out;
    };
    for item in items {
        let Some(doc_obj) = item.get("document").and_then(|v| v.as_object()) else {
            continue;
        };
        let mut doc = HashMap::new();
        if let Some(name) = doc_obj.get("name").and_then(|v| v.as_str()) {
            if let Some(id) = name.rsplit('/').next() {
                doc.insert("id".into(), ok_string(id.to_string()));
            }
        }
        let fields = doc_obj
            .get("fields")
            .and_then(|v| v.as_object())
            .map(decode_fields_object)
            .unwrap_or_default();
        doc.insert("fields".into(), ok_value(Value::Object(fields)));
        out.push(Value::Object(doc).ref_cell());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_primitives() {
        assert_eq!(encode_fs_value(&Value::Nil), "{\"nullValue\":null}");
        assert_eq!(encode_fs_value(&Value::Bool(true)), "{\"booleanValue\":true}");
        assert_eq!(
            encode_fs_value(&Value::Int(42)),
            "{\"integerValue\":\"42\"}"
        );
        assert_eq!(
            encode_fs_value(&Value::String("hi".into())),
            "{\"stringValue\":\"hi\"}"
        );
    }

    #[test]
    fn encode_fields_object() {
        let mut m = HashMap::new();
        m.insert("n".into(), Value::Int(1).ref_cell());
        m.insert("s".into(), Value::String("a".into()).ref_cell());
        let j = encode_fields(&m);
        assert!(j.contains("\"n\":{\"integerValue\":\"1\"}"));
        assert!(j.contains("\"s\":{\"stringValue\":\"a\"}"));
    }

    #[test]
    fn decode_string_and_int() {
        let j = r#"{"fields":{"name":{"stringValue":"Ada"},"age":{"integerValue":"36"}}}"#;
        let m = decode_fields_map(j);
        assert_eq!(
            match &*m.get("name").unwrap().borrow() {
                Value::String(s) => s.as_str(),
                _ => "",
            },
            "Ada"
        );
        assert_eq!(
            match &*m.get("age").unwrap().borrow() {
                Value::Int(n) => *n,
                _ => -1,
            },
            36
        );
    }

    #[test]
    fn encode_unicode() {
        let v = Value::String("日本語".into());
        assert!(encode_fs_value(&v).contains("日本語"));
    }

    #[test]
    fn decode_array_and_map_values() {
        let j = r#"{
            "fields":{
                "tags":{"arrayValue":{"values":[{"stringValue":"a"},{"stringValue":"b"}]}},
                "meta":{"mapValue":{"fields":{"ok":{"booleanValue":true}}}}
            }
        }"#;
        let m = decode_fields_map(j);
        match &*m.get("tags").expect("tags").borrow() {
            Value::Array(a) => assert_eq!(a.len(), 2),
            other => panic!("expected array, got {other:?}"),
        }
        match &*m.get("meta").expect("meta").borrow() {
            Value::Object(obj) => assert!(obj.contains_key("ok")),
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn parse_query_docs_decodes_ids_and_fields() {
        let j = r#"[{
            "document":{
                "name":"projects/p/databases/(default)/documents/users/u1",
                "fields":{"name":{"stringValue":"Ada"}}
            }
        }]"#;
        let docs = parse_query_docs(j);
        assert_eq!(docs.len(), 1);
        let doc = docs[0].borrow();
        let Value::Object(map) = &*doc else {
            panic!("expected doc object");
        };
        let id = map.get("id").expect("id").borrow();
        assert!(matches!(&*id, Value::String(s) if s == "u1"));
    }
}
