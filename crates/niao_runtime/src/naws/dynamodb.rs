//! naws DynamoDB operations: put_item, get_item, delete_item, query.

use super::{aws_error, get_config, ok_bool, ok_string, ok_value, AwsResult};
use crate::{Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;

use super::sigv4::{now_amz, sign, SignInput};
use niao_http::Response as HttpResponse;

// ── DynamoDB AttributeValue conversion ───────────────────────────────────────

/// Convert a Niao `Value` to a DynamoDB AttributeValue JSON fragment.
fn to_attr(val: &Value) -> String {
    match val {
        Value::String(s) => {
            let escaped = json_escape(s);
            format!("{{\"S\":\"{escaped}\"}}")
        }
        Value::Int(n) => format!("{{\"N\":\"{n}\"}}"),
        Value::Float(f) => {
            // Format without trailing zeros where possible
            let s = if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            };
            format!("{{\"N\":\"{s}\"}}")
        }
        Value::Bool(b) => format!("{{\"BOOL\":{b}}}"),
        Value::Nil => "{\"NULL\":true}".into(),
        Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(|v| to_attr(&v.borrow())).collect();
            format!("{{\"L\":[{}]}}", inner.join(","))
        }
        Value::Object(map) => {
            let inner: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let escaped = json_escape(k);
                    format!("\"{}\":{}", escaped, to_attr(&v.borrow()))
                })
                .collect();
            format!("{{\"M\":{{{}}}}}", inner.join(","))
        }
        _ => "{\"NULL\":true}".into(),
    }
}

/// Convert DynamoDB item `{"FieldName": {"S":"val"}, ...}` → Niao `Value::Object`.
fn from_ddb_item(obj: &serde_json::Value) -> ValueRef {
    let Some(map) = obj.as_object() else {
        return Value::Nil.ref_cell();
    };
    let mut out: HashMap<String, ValueRef> = HashMap::new();
    for (field, av) in map {
        out.insert(field.clone(), from_attr_value(av));
    }
    Value::Object(out).ref_cell()
}

fn from_attr_value(av: &serde_json::Value) -> ValueRef {
    let Some(obj) = av.as_object() else {
        return Value::Nil.ref_cell();
    };
    if let Some(s) = obj.get("S").and_then(|v| v.as_str()) {
        return ok_string(s.to_string());
    }
    if let Some(n) = obj.get("N").and_then(|v| v.as_str()) {
        if let Ok(i) = n.parse::<i64>() {
            return ok_value(Value::Int(i));
        }
        if let Ok(f) = n.parse::<f64>() {
            return ok_value(Value::Float(f));
        }
        return ok_string(n.to_string());
    }
    if let Some(b) = obj.get("BOOL").and_then(|v| v.as_bool()) {
        return ok_value(Value::Bool(b));
    }
    if obj.get("NULL").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Value::Nil.ref_cell();
    }
    if let Some(list) = obj.get("L").and_then(|v| v.as_array()) {
        let items: Vec<ValueRef> = list.iter().map(from_attr_value).collect();
        return Value::Array(items).ref_cell();
    }
    if let Some(m) = obj.get("M").and_then(|v| v.as_object()) {
        let mut inner: HashMap<String, ValueRef> = HashMap::new();
        for (k, v) in m {
            inner.insert(k.clone(), from_attr_value(v));
        }
        return Value::Object(inner).ref_cell();
    }
    Value::Nil.ref_cell()
}

// ── item/key JSON builders ────────────────────────────────────────────────────

fn build_item_json(niao_obj: &HashMap<String, ValueRef>) -> String {
    let fields: Vec<String> = niao_obj
        .iter()
        .map(|(k, v)| {
            let escaped = json_escape(k);
            format!("\"{}\":{}", escaped, to_attr(&v.borrow()))
        })
        .collect();
    format!("{{{}}}", fields.join(","))
}

// ── HTTP helpers ─────────────────────────────────────────────────────────────

fn ddb_endpoint(region: &str) -> String {
    format!("https://dynamodb.{region}.amazonaws.com/")
}

fn ddb_request(
    cfg: &super::AwsConfig,
    target: &str,
    body_str: &str,
    span: Span,
) -> Result<HttpResponse, crate::RuntimeError> {
    let endpoint = ddb_endpoint(&cfg.region);
    let host = format!("dynamodb.{}.amazonaws.com", cfg.region);
    let body_bytes = body_str.as_bytes();
    let (amz_dt, amz_d) = now_amz();

    let ct = "application/x-amz-json-1.0";
    let extra = [("content-type", ct), ("x-amz-target", target)];

    let inp = SignInput {
        method: "POST",
        host: &host,
        path: "/",
        query: "",
        region: &cfg.region,
        service: "dynamodb",
        access_key: &cfg.access_key,
        secret_key: &cfg.secret_key,
        session_token: cfg.session_token.as_deref(),
        body: body_bytes,
        amz_datetime: &amz_dt,
        amz_date: &amz_d,
        extra_headers: &extra,
    };
    let signed = sign(&inp);

    let mut builder = niao_http::post(&endpoint);
    for (k, v) in &signed.headers {
        builder = builder.set(k.clone(), v.clone());
    }
    builder = builder.set("Content-Type", ct);
    builder = builder.set("X-Amz-Target", target);

    builder.send_string(body_str).map_err(|e| {
        crate::RuntimeError::at(
            span,
            codes::E2801_NAWS_ERROR,
            format!("DynamoDB request failed: {e}"),
        )
    })
}

// ── public API ────────────────────────────────────────────────────────────────

/// `naws.dynamodb_put(config_id, table, item{}) → true`
pub fn dynamodb_put(args: &[ValueRef], span: Span) -> AwsResult {
    if args.len() != 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E2800_NAWS_ARITY,
            "naws_dynamodb_put() expects 3 arguments: config, table, item",
        ));
    }
    let config_id = super::int_arg(args, 0, "naws_dynamodb_put", span)?;
    let cfg = get_config(config_id, span)?;
    let table = super::str_arg(args, 1, "naws_dynamodb_put", span)?;
    let item_map = super::obj_arg(args, 2, "naws_dynamodb_put", span)?;

    let item_json = build_item_json(&item_map);
    let body = format!(
        "{{\"TableName\":\"{}\",\"Item\":{}}}",
        json_escape(&table),
        item_json
    );

    match ddb_request(&cfg, "DynamoDB_20120810.PutItem", &body, span) {
        Ok(resp) => {
            if resp.status >= 400 {
                let msg = String::from_utf8_lossy(&resp.body).into_owned();
                return Ok(aws_error(
                    codes::E2801_NAWS_ERROR,
                    "naws_dynamodb_error",
                    msg,
                    span,
                ));
            }
            Ok(ok_bool(true))
        }
        Err(e) => Err(e),
    }
}

/// `naws.dynamodb_get(config_id, table, key{}) → item{} | nil`
pub fn dynamodb_get(args: &[ValueRef], span: Span) -> AwsResult {
    if args.len() != 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E2800_NAWS_ARITY,
            "naws_dynamodb_get() expects 3 arguments: config, table, key",
        ));
    }
    let config_id = super::int_arg(args, 0, "naws_dynamodb_get", span)?;
    let cfg = get_config(config_id, span)?;
    let table = super::str_arg(args, 1, "naws_dynamodb_get", span)?;
    let key_map = super::obj_arg(args, 2, "naws_dynamodb_get", span)?;

    let key_json = build_item_json(&key_map);
    let body = format!(
        "{{\"TableName\":\"{}\",\"Key\":{}}}",
        json_escape(&table),
        key_json
    );

    match ddb_request(&cfg, "DynamoDB_20120810.GetItem", &body, span) {
        Ok(resp) => {
            if resp.status >= 400 {
                let msg = String::from_utf8_lossy(&resp.body).into_owned();
                return Ok(aws_error(
                    codes::E2801_NAWS_ERROR,
                    "naws_dynamodb_error",
                    msg,
                    span,
                ));
            }
            let body_str = String::from_utf8_lossy(&resp.body).into_owned();
            match serde_json::from_str::<serde_json::Value>(&body_str) {
                Ok(json) => {
                    if let Some(item) = json.get("Item") {
                        Ok(from_ddb_item(item))
                    } else {
                        Ok(Value::Nil.ref_cell())
                    }
                }
                Err(e) => Ok(aws_error(
                    codes::E2801_NAWS_ERROR,
                    "naws_dynamodb_error",
                    format!("JSON parse error: {e}"),
                    span,
                )),
            }
        }
        Err(e) => Err(e),
    }
}

/// `naws.dynamodb_delete(config_id, table, key{}) → true`
pub fn dynamodb_delete(args: &[ValueRef], span: Span) -> AwsResult {
    if args.len() != 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E2800_NAWS_ARITY,
            "naws_dynamodb_delete() expects 3 arguments: config, table, key",
        ));
    }
    let config_id = super::int_arg(args, 0, "naws_dynamodb_delete", span)?;
    let cfg = get_config(config_id, span)?;
    let table = super::str_arg(args, 1, "naws_dynamodb_delete", span)?;
    let key_map = super::obj_arg(args, 2, "naws_dynamodb_delete", span)?;

    let key_json = build_item_json(&key_map);
    let body = format!(
        "{{\"TableName\":\"{}\",\"Key\":{}}}",
        json_escape(&table),
        key_json
    );

    match ddb_request(&cfg, "DynamoDB_20120810.DeleteItem", &body, span) {
        Ok(resp) => {
            if resp.status >= 400 {
                let msg = String::from_utf8_lossy(&resp.body).into_owned();
                return Ok(aws_error(
                    codes::E2801_NAWS_ERROR,
                    "naws_dynamodb_error",
                    msg,
                    span,
                ));
            }
            Ok(ok_bool(true))
        }
        Err(e) => Err(e),
    }
}

/// `naws.dynamodb_query(config_id, table, opts{}) → items[]`
///
/// `opts` fields (all optional):
/// - `key_condition`: KeyConditionExpression string
/// - `filter`: FilterExpression string
/// - `names`: ExpressionAttributeNames `{}`
/// - `values`: ExpressionAttributeValues `{}`
/// - `index`: IndexName
/// - `limit`: max items (int)
/// - `ascending`: scan order (bool, default true)
pub fn dynamodb_query(args: &[ValueRef], span: Span) -> AwsResult {
    if args.len() != 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E2800_NAWS_ARITY,
            "naws_dynamodb_query() expects 3 arguments: config, table, opts",
        ));
    }
    let config_id = super::int_arg(args, 0, "naws_dynamodb_query", span)?;
    let cfg = get_config(config_id, span)?;
    let table = super::str_arg(args, 1, "naws_dynamodb_query", span)?;
    let opts = super::obj_arg(args, 2, "naws_dynamodb_query", span)?;

    let mut body_map: HashMap<&str, String> = HashMap::new();
    body_map.insert("TableName", format!("\"{}\"", json_escape(&table)));

    if let Some(kce) = opts.get("key_condition") {
        if let Value::String(s) = &*kce.borrow() {
            body_map.insert("KeyConditionExpression", format!("\"{}\"", json_escape(s)));
        }
    }
    if let Some(fe) = opts.get("filter") {
        if let Value::String(s) = &*fe.borrow() {
            body_map.insert("FilterExpression", format!("\"{}\"", json_escape(s)));
        }
    }
    if let Some(idx) = opts.get("index") {
        if let Value::String(s) = &*idx.borrow() {
            body_map.insert("IndexName", format!("\"{}\"", json_escape(s)));
        }
    }
    if let Some(lim) = opts.get("limit") {
        if let Value::Int(n) = &*lim.borrow() {
            body_map.insert("Limit", format!("{n}"));
        }
    }
    if let Some(asc) = opts.get("ascending") {
        if let Value::Bool(b) = &*asc.borrow() {
            body_map.insert("ScanIndexForward", if *b { "true" } else { "false" }.into());
        }
    }
    if let Some(names) = opts.get("names") {
        if let Value::Object(nm) = &*names.borrow() {
            let inner: Vec<String> = nm
                .iter()
                .map(|(k, v)| {
                    let val = match &*v.borrow() {
                        Value::String(s) => format!("\"{}\"", json_escape(s)),
                        _ => "null".into(),
                    };
                    format!("\"{}\":{}", json_escape(k), val)
                })
                .collect();
            body_map.insert(
                "ExpressionAttributeNames",
                format!("{{{}}}", inner.join(",")),
            );
        }
    }
    if let Some(values) = opts.get("values") {
        if let Value::Object(vm) = &*values.borrow() {
            let inner: Vec<String> = vm
                .iter()
                .map(|(k, v)| format!("\"{}\":{}", json_escape(k), to_attr(&v.borrow())))
                .collect();
            body_map.insert(
                "ExpressionAttributeValues",
                format!("{{{}}}", inner.join(",")),
            );
        }
    }

    let body_fields: Vec<String> = body_map
        .iter()
        .map(|(k, v)| format!("\"{}\":{}", k, v))
        .collect();
    let body = format!("{{{}}}", body_fields.join(","));

    match ddb_request(&cfg, "DynamoDB_20120810.Query", &body, span) {
        Ok(resp) => {
            if resp.status >= 400 {
                let msg = String::from_utf8_lossy(&resp.body).into_owned();
                return Ok(aws_error(
                    codes::E2801_NAWS_ERROR,
                    "naws_dynamodb_error",
                    msg,
                    span,
                ));
            }
            let body_str = String::from_utf8_lossy(&resp.body).into_owned();
            match serde_json::from_str::<serde_json::Value>(&body_str) {
                Ok(json) => {
                    let items = json
                        .get("Items")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .map(|item| from_ddb_item(item))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    Ok(Value::Array(items).ref_cell())
                }
                Err(e) => Ok(aws_error(
                    codes::E2801_NAWS_ERROR,
                    "naws_dynamodb_error",
                    format!("JSON parse error: {e}"),
                    span,
                )),
            }
        }
        Err(e) => Err(e),
    }
}

// ── internal ──────────────────────────────────────────────────────────────────

fn json_escape(s: &str) -> String {
    super::json_escape(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_attr_string() {
        assert_eq!(to_attr(&Value::String("hello".into())), r#"{"S":"hello"}"#);
    }

    #[test]
    fn to_attr_int() {
        assert_eq!(to_attr(&Value::Int(42)), r#"{"N":"42"}"#);
    }

    #[test]
    fn to_attr_bool() {
        assert_eq!(to_attr(&Value::Bool(true)), r#"{"BOOL":true}"#);
    }

    #[test]
    fn to_attr_nil() {
        assert_eq!(to_attr(&Value::Nil), r#"{"NULL":true}"#);
    }

    #[test]
    fn from_attr_string() {
        let av = serde_json::json!({"S": "world"});
        match &*from_attr_value(&av).borrow() {
            Value::String(s) => assert_eq!(s, "world"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn from_attr_number_int() {
        let av = serde_json::json!({"N": "123"});
        match &*from_attr_value(&av).borrow() {
            Value::Int(n) => assert_eq!(*n, 123),
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_object_item() {
        let mut item = HashMap::new();
        item.insert("name".to_string(), ok_string("Alice".to_string()));
        item.insert("age".to_string(), ok_value(Value::Int(30)));
        let json_str = build_item_json(&item);
        // Verify the JSON contains expected attribute values
        assert!(json_str.contains("\"S\":\"Alice\"") || json_str.contains(r#""S":"Alice""#));
        assert!(json_str.contains("\"N\":\"30\""));
    }
}
