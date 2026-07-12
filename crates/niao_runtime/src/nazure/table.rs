//! Azure Table Storage REST operations.
//!
//! Endpoint: `https://{account}.table.core.windows.net`
//! API version: 2019-02-02
//! Auth: SharedKeyLite (simpler string-to-sign, suitable for Table REST).
//!
//! Entities are represented as Niao `Value::Object` maps.

use super::{auth, AzureConfig};
use crate::{Value, ValueRef};
use crate::error_value;
use niao_errors::codes;
use niao_ast::Span;
use niao_bignum::BigInt;
use niao_json_core::{Number as JNumber, Value as JValue};
use std::collections::HashMap;

const TABLE_VERSION: &str = "2019-02-02";

fn table_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2811_NAZURE_ERROR, "nazure_error", msg.into(), span)
}

fn auth_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2813_NAZURE_AUTH, "nazure_error", msg.into(), span)
}

fn table_url(account: &str, path: &str, query: Option<&str>) -> String {
    let base = format!("https://{}.table.core.windows.net/{}", account, path);
    match query {
        Some(q) if !q.is_empty() => format!("{base}?{q}"),
        _ => base,
    }
}

/// Build authorization header using SharedKeyLite for Table, or Bearer.
fn make_table_auth(
    cfg: &AzureConfig,
    date: &str,
    canon_resource: &str,
) -> Result<Option<String>, String> {
    if let Some(key) = &cfg.key {
        Ok(Some(auth::shared_key_lite_table(
            &cfg.account,
            key,
            date,
            canon_resource,
        )))
    } else if let (Some(tenant), Some(cid), Some(csec)) =
        (&cfg.tenant, &cfg.client_id, &cfg.client_secret)
    {
        let scope = format!(
            "https://{}.table.core.windows.net/.default",
            cfg.account
        );
        let token = auth::fetch_bearer_token(tenant, cid, csec, &scope)?;
        Ok(Some(format!("Bearer {token}")))
    } else {
        Ok(None)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// JSON ↔ Niao value conversion (Table entities are JSON)
// ──────────────────────────────────────────────────────────────────────────────

fn jval_to_niao(j: JValue) -> Value {
    match j {
        JValue::Null => Value::Nil,
        JValue::Bool(b) => Value::Bool(b),
        JValue::Number(n) => match n {
            JNumber::I64(i) => Value::Int(i),
            JNumber::U64(u) if u <= i64::MAX as u64 => Value::Int(u as i64),
            JNumber::U64(u) => Value::BigInt(BigInt::from(u)),
            JNumber::F64(f) => {
                if f.fract() == 0.0 && f.is_finite() && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                    Value::Int(f as i64)
                } else {
                    Value::Float(f)
                }
            }
        },
        JValue::String(s) => Value::String(s),
        JValue::Array(arr) => {
            Value::Array(arr.into_iter().map(|v| jval_to_niao(v).ref_cell()).collect())
        }
        JValue::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, v) in map.iter() {
                out.insert(k.to_string(), jval_to_niao(v.clone()).ref_cell());
            }
            Value::Object(out)
        }
    }
}

fn parse_json_to_value(json: &str, span: Span) -> Result<Value, ValueRef> {
    match niao_json_core::parse(json) {
        Ok(j) => Ok(jval_to_niao(j)),
        Err(e) => Err(table_error(span, format!("nazure: JSON parse error: {e:?}"))),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Table INSERT (POST)
// ──────────────────────────────────────────────────────────────────────────────

/// Insert a single entity. `entity_json` is the JSON-serialised entity object.
pub fn table_insert(
    cfg: &AzureConfig,
    table: &str,
    entity_json: &str,
    span: Span,
) -> ValueRef {
    let date = auth::rfc1123_now();
    let canon = format!("/{}/{}", cfg.account, table);
    let auth_hdr = match make_table_auth(cfg, &date, &canon) {
        Ok(v) => v,
        Err(e) => return auth_error(span, e),
    };

    let url = table_url(&cfg.account, table, None);
    let mut req = niao_http::post(&url)
        .set("x-ms-date", &date)
        .set("x-ms-version", TABLE_VERSION)
        .set("Accept", "application/json;odata=nometadata")
        .set("Content-Type", "application/json")
        .set("Prefer", "return-content");
    if let Some(h) = auth_hdr {
        req = req.set("Authorization", h);
    }

    match req.send_string(entity_json) {
        Err(e) => table_error(span, format!("nazure table_insert: {e}")),
        Ok(resp) => {
            let status = resp.status;
            let body = String::from_utf8_lossy(&resp.body).into_owned();
            if !(200..300).contains(&status) {
                return table_error(span, format!("table_insert HTTP {status}: {body}"));
            }
            if body.trim().is_empty() {
                return Value::Object(HashMap::new()).ref_cell();
            }
            match parse_json_to_value(&body, span) {
                Ok(v) => v.ref_cell(),
                Err(e) => e,
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Table QUERY (GET)
// ──────────────────────────────────────────────────────────────────────────────

/// Query entities. Returns a `Value::Array` of entity objects.
/// `filter` is an optional OData filter expression, e.g. `"PartitionKey eq 'Alice'"`.
pub fn table_query(
    cfg: &AzureConfig,
    table: &str,
    filter: Option<&str>,
    span: Span,
) -> ValueRef {
    let date = auth::rfc1123_now();
    let canon = format!("/{}/{}", cfg.account, table);
    let auth_hdr = match make_table_auth(cfg, &date, &canon) {
        Ok(v) => v,
        Err(e) => return auth_error(span, e),
    };

    let query = filter.filter(|f| !f.is_empty()).map(|f| {
        format!("$filter={}", niao_http::percent_encode(f.as_bytes()))
    });
    let url = table_url(&cfg.account, table, query.as_deref());
    let mut req = niao_http::get(&url)
        .set("x-ms-date", &date)
        .set("x-ms-version", TABLE_VERSION)
        .set("Accept", "application/json;odata=nometadata");
    if let Some(h) = auth_hdr {
        req = req.set("Authorization", h);
    }

    match req.send() {
        Err(e) => table_error(span, format!("nazure table_query: {e}")),
        Ok(resp) => {
            let status = resp.status;
            let body = String::from_utf8_lossy(&resp.body).into_owned();
            if !(200..300).contains(&status) {
                return table_error(span, format!("table_query HTTP {status}: {body}"));
            }
            // Response: {"value":[{...},{...}]}
            match niao_json_core::parse(&body) {
                Err(e) => table_error(span, format!("nazure: JSON parse error: {e:?}")),
                Ok(JValue::Object(map)) => {
                    let rows = map.get("value").cloned().unwrap_or(JValue::Array(vec![]));
                    jval_to_niao(rows).ref_cell()
                }
                Ok(other) => jval_to_niao(other).ref_cell(),
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Table DELETE (DELETE)
// ──────────────────────────────────────────────────────────────────────────────

/// Delete a single entity identified by `partition_key` and `row_key`.
pub fn table_delete(
    cfg: &AzureConfig,
    table: &str,
    partition_key: &str,
    row_key: &str,
    span: Span,
) -> ValueRef {
    let date = auth::rfc1123_now();
    // Entity path: {table}(PartitionKey='{pk}',RowKey='{rk}')
    let entity_path = format!(
        "{}(PartitionKey='{}',RowKey='{}')",
        table, partition_key, row_key
    );
    let canon = format!("/{}/{}", cfg.account, entity_path);
    let auth_hdr = match make_table_auth(cfg, &date, &canon) {
        Ok(v) => v,
        Err(e) => return auth_error(span, e),
    };

    let url = table_url(&cfg.account, &entity_path, None);
    let mut req = niao_http::delete(&url)
        .set("x-ms-date", &date)
        .set("x-ms-version", TABLE_VERSION)
        .set("If-Match", "*");
    if let Some(h) = auth_hdr {
        req = req.set("Authorization", h);
    }

    match req.send() {
        Err(e) => table_error(span, format!("nazure table_delete: {e}")),
        Ok(resp) => {
            let status = resp.status;
            if status == 204 || status == 200 {
                Value::Bool(true).ref_cell()
            } else {
                let body = String::from_utf8_lossy(&resp.body).into_owned();
                table_error(span, format!("table_delete HTTP {status}: {body}"))
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jval_to_niao_primitives() {
        assert!(matches!(jval_to_niao(JValue::Null), Value::Nil));
        assert!(matches!(jval_to_niao(JValue::Bool(true)), Value::Bool(true)));
        assert!(matches!(
            jval_to_niao(JValue::Number(JNumber::I64(42))),
            Value::Int(42)
        ));
        assert!(matches!(
            jval_to_niao(JValue::String("hello".into())),
            Value::String(s) if s == "hello"
        ));
    }

    #[test]
    fn jval_to_niao_array() {
        let arr = JValue::Array(vec![JValue::Bool(false), JValue::Bool(true)]);
        match jval_to_niao(arr) {
            Value::Array(items) => assert_eq!(items.len(), 2),
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn jval_to_niao_object() {
        use niao_json_core::parse;
        let j = parse(r#"{"PartitionKey":"pk","RowKey":"rk","score":99}"#).unwrap();
        match jval_to_niao(j) {
            Value::Object(map) => {
                assert!(map.contains_key("PartitionKey"));
                assert!(map.contains_key("score"));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
