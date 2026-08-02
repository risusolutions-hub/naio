//! Value <-> MySQL type mapping.

use crate::Value;
use mysql::Value as MysqlValue;
use niao_ast::Span;
use niao_errors::codes;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum BoundValue {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
    Blob(Vec<u8>),
    Json(JsonValue),
}

pub fn niao_to_bound(val: &Value, span: Span) -> Result<BoundValue, crate::RuntimeError> {
    match val {
        Value::Nil => Ok(BoundValue::Null),
        Value::Int(n) => Ok(BoundValue::Int(*n)),
        Value::Float(f) => Ok(BoundValue::Float(*f)),
        Value::Bool(b) => Ok(BoundValue::Bool(*b)),
        Value::String(s) => Ok(BoundValue::Text(s.clone())),
        Value::ByteArray(b) => Ok(BoundValue::Blob(b.clone())),
        Value::Object(map) => {
            let mut json_map = serde_json::Map::new();
            for (k, v) in map {
                json_map.insert(k.clone(), niao_to_json(&*v.borrow())?);
            }
            Ok(BoundValue::Json(JsonValue::Object(json_map)))
        }
        Value::Array(items) => {
            let mut arr = Vec::with_capacity(items.len());
            for item in items {
                arr.push(niao_to_json(&*item.borrow())?);
            }
            Ok(BoundValue::Json(JsonValue::Array(arr)))
        }
        other => Err(crate::RuntimeError::at(
            span,
            codes::E1916_NMYSQL_BIND,
            format!(
                "cannot bind value of type {} to MySQL parameter",
                other.type_name()
            ),
        )),
    }
}

fn niao_to_json(val: &Value) -> Result<JsonValue, crate::RuntimeError> {
    match val {
        Value::Nil => Ok(JsonValue::Null),
        Value::Int(n) => Ok(JsonValue::from(*n)),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(JsonValue::Number)
            .ok_or_else(|| crate::RuntimeError::TypeError {
                message: "float cannot be represented as JSON number".into(),
                line: 0,
                col: 0,
            }),
        Value::Bool(b) => Ok(JsonValue::Bool(*b)),
        Value::String(s) => Ok(JsonValue::String(s.clone())),
        Value::Array(items) => {
            let mut arr = Vec::with_capacity(items.len());
            for item in items {
                arr.push(niao_to_json(&*item.borrow())?);
            }
            Ok(JsonValue::Array(arr))
        }
        Value::Object(map) => {
            let mut json_map = serde_json::Map::new();
            for (k, v) in map {
                json_map.insert(k.clone(), niao_to_json(&*v.borrow())?);
            }
            Ok(JsonValue::Object(json_map))
        }
        other => Ok(JsonValue::String(other.type_name().to_string())),
    }
}

pub fn bound_to_mysql(params: &[BoundValue]) -> Vec<MysqlValue> {
    params
        .iter()
        .map(|p| match p {
            BoundValue::Null => MysqlValue::NULL,
            BoundValue::Int(n) => MysqlValue::Int(*n),
            BoundValue::Float(f) => MysqlValue::Double(*f),
            BoundValue::Bool(b) => MysqlValue::Int(if *b { 1 } else { 0 }),
            BoundValue::Text(s) => MysqlValue::Bytes(s.as_bytes().to_vec()),
            BoundValue::Blob(b) => MysqlValue::Bytes(b.clone()),
            BoundValue::Json(j) => MysqlValue::Bytes(j.to_string().into_bytes()),
        })
        .collect()
}

/// Rewrite `$1`, `$2`, … to MySQL `?`. Existing `?` placeholders are preserved.
pub fn rewrite_placeholders(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' && !in_double && !in_backtick {
            if in_single && i + 1 < chars.len() && chars[i + 1] == '\'' {
                out.push(c);
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            in_single = !in_single;
            out.push(c);
        } else if c == '"' && !in_single && !in_backtick {
            in_double = !in_double;
            out.push(c);
        } else if c == '`' && !in_single && !in_double {
            in_backtick = !in_backtick;
            out.push(c);
        } else if c == '$' && !in_single && !in_double && !in_backtick {
            let mut j = i + 1;
            if j < chars.len() && chars[j].is_ascii_digit() {
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                out.push('?');
                i = j;
                continue;
            }
            out.push(c);
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

pub fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

pub fn quote_literal(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
}

pub fn mysql_to_niao(val: MysqlValue) -> Value {
    match val {
        MysqlValue::NULL => Value::Nil,
        MysqlValue::Int(n) => Value::Int(n),
        MysqlValue::UInt(n) => {
            if n <= i64::MAX as u64 {
                Value::Int(n as i64)
            } else {
                Value::String(n.to_string())
            }
        }
        MysqlValue::Float(f) => Value::Float(f as f64),
        MysqlValue::Double(f) => Value::Float(f),
        MysqlValue::Bytes(b) => {
            if let Ok(s) = std::str::from_utf8(&b) {
                if let Ok(j) = serde_json::from_str::<JsonValue>(s) {
                    if s.starts_with('{') || s.starts_with('[') {
                        return json_to_niao(&j);
                    }
                }
                Value::String(s.to_string())
            } else {
                Value::ByteArray(b)
            }
        }
        MysqlValue::Date(y, m, d, h, mi, s, us) => {
            Value::String(format!(
                "{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}.{:06}",
                us
            ))
        }
        MysqlValue::Time(neg, d, h, mi, s, us) => {
            let sign = if neg { "-" } else { "" };
            Value::String(format!(
                "{sign}{d} {h:02}:{mi:02}:{s:02}.{:06}",
                us
            ))
        }
    }
}

fn json_to_niao(j: &JsonValue) -> Value {
    match j {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::String(n.to_string())
            }
        }
        JsonValue::String(s) => Value::String(s.clone()),
        JsonValue::Array(items) => {
            Value::Array(items.iter().map(|v| json_to_niao(v).ref_cell()).collect())
        }
        JsonValue::Object(map) => {
            let mut out = HashMap::new();
            for (k, v) in map {
                out.insert(k.clone(), json_to_niao(v).ref_cell());
            }
            Value::Object(out)
        }
    }
}

pub fn value_to_async(val: &Value) -> crate::async_tasks::AsyncValue {
    use crate::async_tasks::AsyncValue;
    match val {
        Value::Nil => AsyncValue::nil(),
        Value::Int(n) => AsyncValue::int(*n),
        Value::Bool(b) => AsyncValue::Bool(*b),
        Value::Float(f) => AsyncValue::Float(*f),
        Value::String(s) => AsyncValue::String(s.clone()),
        Value::IntArray(v) => AsyncValue::IntArray(v.clone()),
        Value::ByteArray(v) => AsyncValue::ByteArray(v.clone()),
        Value::Array(items) => {
            AsyncValue::Array(items.iter().map(|v| value_to_async(&*v.borrow())).collect())
        }
        Value::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k.clone(), value_to_async(&*v.borrow()));
            }
            AsyncValue::Object(out)
        }
        other => AsyncValue::String(other.type_name().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_dollar_to_qmark() {
        assert_eq!(
            rewrite_placeholders("SELECT * FROM t WHERE a = $1 AND b = $2"),
            "SELECT * FROM t WHERE a = ? AND b = ?"
        );
    }

    #[test]
    fn rewrite_preserves_existing_qmark() {
        assert_eq!(
            rewrite_placeholders("SELECT ? FROM t WHERE x = $1"),
            "SELECT ? FROM t WHERE x = ?"
        );
    }

    #[test]
    fn rewrite_skips_string_literals() {
        assert_eq!(
            rewrite_placeholders("SELECT '$1' , $1"),
            "SELECT '$1' , ?"
        );
    }

    #[test]
    fn quote_ident_escapes_backticks() {
        assert_eq!(quote_ident("a`b"), "`a``b`");
    }

    #[test]
    fn quote_literal_escapes_quotes() {
        assert_eq!(quote_literal("a'b"), "'a''b'");
    }
}
