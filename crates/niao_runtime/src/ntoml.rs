//! Native TOML standard library — parse/stringify via `niao_json_core::toml`.
//!
//! Import with `import "ntoml"` (or `import "std/ntoml"`).

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_bignum::BigInt;
use niao_json_core::toml::parse_to_value;
use niao_json_core::{Number as JNumber, Value as JsonValue};
use std::collections::HashMap;
use std::rc::Rc;

// codes.rs integration pending — use local constants until wired.
const E2840_NTOML_ARITY: u32 = 2840;
const E2841_NTOML_ERROR: u32 = 2841;
const E2842_NTOML_TYPE: u32 = 2842;
const E2843_NTOML_PARSE: u32 = 2843;

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::TypeError {
        message: msg.into(),
        line: span.line,
        col: span.col,
    }
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E2840_NTOML_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// JsonValue ↔ Niao Value bridge (same pattern as json.rs)
// ---------------------------------------------------------------------------

fn json_to_value(j: JsonValue) -> Value {
    match j {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(b) => Value::Bool(b),
        JsonValue::Number(n) => match n {
            JNumber::I64(i) => Value::Int(i),
            JNumber::U64(u) if u <= i64::MAX as u64 => Value::Int(u as i64),
            JNumber::U64(u) => Value::BigInt(BigInt::from(u)),
            JNumber::F64(f) => {
                if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                    Value::Int(f as i64)
                } else {
                    Value::Float(f)
                }
            }
        },
        JsonValue::String(s) => Value::String(s),
        JsonValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(json_to_value(item).ref_cell());
            }
            Value::Array(out)
        }
        JsonValue::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, v) in map.iter() {
                out.insert(k.to_string(), json_to_value(v.clone()).ref_cell());
            }
            Value::Object(out)
        }
    }
}

fn parse_toml_text(text: &str, span: Span) -> NiaoResult<Value> {
    parse_to_value(text)
        .map(json_to_value)
        .map_err(|e| RuntimeError::at(span, E2843_NTOML_PARSE, format!("ntoml_parse: {e}")))
}

// ---------------------------------------------------------------------------
// Minimal TOML emitter
// ---------------------------------------------------------------------------

fn is_bare_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn append_quoted_key(key: &str, out: &mut String) {
    if is_bare_key(key) {
        out.push_str(key);
    } else {
        out.push('"');
        append_escaped_string(key, out);
        out.push('"');
    }
}

fn append_escaped_string(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
}

fn append_scalar(v: &Value, out: &mut String, span: Span) -> NiaoResult<()> {
    match v {
        Value::Nil => {
            return Err(RuntimeError::at(
                span,
                E2842_NTOML_TYPE,
                "ntoml_stringify: nil cannot be written to TOML",
            ));
        }
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::BigInt(n) => {
            if let Some(i) = n.to_i64() {
                out.push_str(&i.to_string());
            } else if let Some(u) = n.to_u64() {
                out.push_str(&u.to_string());
            } else {
                return Err(RuntimeError::at(
                    span,
                    E2842_NTOML_TYPE,
                    format!("ntoml_stringify: bigint {n} does not fit in TOML number"),
                ));
            }
        }
        Value::Float(f) => {
            if !f.is_finite() {
                return Err(RuntimeError::at(
                    span,
                    E2842_NTOML_TYPE,
                    "ntoml_stringify: non-finite float cannot be written to TOML",
                ));
            }
            out.push_str(&JNumber::F64(*f).to_string());
        }
        Value::String(s) => {
            out.push('"');
            append_escaped_string(s, out);
            out.push('"');
        }
        other => {
            return Err(RuntimeError::at(
                span,
                E2842_NTOML_TYPE,
                format!(
                    "ntoml_stringify: expected scalar, got {}",
                    other.type_name()
                ),
            ));
        }
    }
    Ok(())
}

fn is_scalar_value(v: &Value) -> bool {
    matches!(
        v,
        Value::Nil
            | Value::Bool(_)
            | Value::Int(_)
            | Value::BigInt(_)
            | Value::Float(_)
            | Value::String(_)
    )
}

fn is_inline_array(v: &Value) -> bool {
    match v {
        Value::IntArray(_)
        | Value::FloatArray(_)
        | Value::BoolArray(_)
        | Value::ByteArray(_)
        | Value::StringArray(_) => true,
        Value::Array(items) => {
            !items.is_empty() && items.iter().all(|slot| is_scalar_value(&slot.borrow()))
        }
        _ => false,
    }
}

fn is_array_of_tables(v: &Value) -> bool {
    match v {
        Value::Array(items) if !items.is_empty() => items
            .iter()
            .all(|slot| matches!(&*slot.borrow(), Value::Object(_))),
        _ => false,
    }
}

fn append_inline_array(v: &Value, out: &mut String, span: Span) -> NiaoResult<()> {
    out.push('[');
    match v {
        Value::IntArray(items) => {
            for (i, &n) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&n.to_string());
            }
        }
        Value::FloatArray(items) => {
            for (i, &f) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                append_scalar(&Value::Float(f), out, span)?;
            }
        }
        Value::BoolArray(items) => {
            for (i, &b) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(if b != 0 { "true" } else { "false" });
            }
        }
        Value::ByteArray(items) => {
            for (i, &b) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&(b as i64).to_string());
            }
        }
        Value::StringArray(items) => {
            for i in 0..items.len() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push('"');
                append_escaped_string(&items.get(i).unwrap_or_default(), out);
                out.push('"');
            }
        }
        Value::Array(items) => {
            for (i, slot) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                append_scalar(&slot.borrow(), out, span)?;
            }
        }
        other => {
            return Err(RuntimeError::at(
                span,
                E2842_NTOML_TYPE,
                format!(
                    "ntoml_stringify: expected inline array, got {}",
                    other.type_name()
                ),
            ));
        }
    }
    out.push(']');
    Ok(())
}

fn table_header(path: &[String], array: bool) -> String {
    let joined = path.join(".");
    if array {
        format!("[[{joined}]]")
    } else {
        format!("[{joined}]")
    }
}

fn emit_table_rows(
    map: &HashMap<String, ValueRef>,
    path: &[String],
    out: &mut String,
    pretty: bool,
    span: Span,
) -> NiaoResult<()> {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();

    let mut scalars: Vec<&String> = Vec::with_capacity(keys.len());
    let mut inline_arrays: Vec<&String> = Vec::with_capacity(keys.len());
    let mut nested_tables: Vec<&String> = Vec::with_capacity(keys.len());
    let mut array_tables: Vec<&String> = Vec::with_capacity(keys.len());
    let mut unsupported: Vec<&String> = Vec::new();

    for key in keys.iter() {
        let v = &*map[*key].borrow();
        if is_scalar_value(v) {
            scalars.push(key);
        } else if is_inline_array(v) {
            inline_arrays.push(key);
        } else if matches!(v, Value::Object(_)) {
            nested_tables.push(key);
        } else if is_array_of_tables(v) {
            array_tables.push(key);
        } else {
            unsupported.push(key);
        }
    }

    if !unsupported.is_empty() {
        let key = unsupported[0];
        let v = &*map[key].borrow();
        return Err(RuntimeError::at(
            span,
            E2842_NTOML_TYPE,
            format!(
                "ntoml_stringify: cannot encode key '{key}' with type {}",
                v.type_name()
            ),
        ));
    }

    for key in &scalars {
        append_quoted_key(key, out);
        out.push_str(" = ");
        append_scalar(&*map[(*key).as_str()].borrow(), out, span)?;
        out.push('\n');
    }

    for key in &inline_arrays {
        append_quoted_key(key, out);
        out.push_str(" = ");
        append_inline_array(&*map[(*key).as_str()].borrow(), out, span)?;
        out.push('\n');
    }

    let needs_section_gap = pretty && (!scalars.is_empty() || !inline_arrays.is_empty());

    for (i, key) in nested_tables.iter().copied().enumerate() {
        if needs_section_gap && i == 0 {
            out.push('\n');
        } else if pretty && i > 0 {
            out.push('\n');
        }
        let mut next_path = path.to_vec();
        next_path.push(key.to_string());
        out.push_str(&table_header(&next_path, false));
        out.push('\n');
        if let Value::Object(nested) = &*map[key].borrow() {
            emit_table_rows(nested, &next_path, out, pretty, span)?;
        }
    }

    for (i, key) in array_tables.iter().copied().enumerate() {
        if pretty && (needs_section_gap || !nested_tables.is_empty()) && i == 0 {
            out.push('\n');
        }
        if let Value::Array(items) = &*map[key].borrow() {
            let mut next_path = path.to_vec();
            next_path.push(key.to_string());
            for (j, slot) in items.iter().enumerate() {
                if pretty && j > 0 {
                    out.push('\n');
                }
                out.push_str(&table_header(&next_path, true));
                out.push('\n');
                if let Value::Object(row) = &*slot.borrow() {
                    emit_table_rows(row, &next_path, out, pretty, span)?;
                }
            }
        }
    }

    Ok(())
}

fn estimate_toml_len(v: &Value) -> usize {
    match v {
        Value::Object(map) => {
            1 + map
                .iter()
                .map(|(k, slot)| k.len() + 4 + estimate_toml_len(&slot.borrow()))
                .sum::<usize>()
        }
        Value::Array(items) => 2 + items.len() * 8,
        Value::String(s) => s.len() + 4,
        Value::Int(_) | Value::Float(_) | Value::Bool(_) => 16,
        _ => 16,
    }
}

fn stringify_toml(v: &Value, pretty: bool, span: Span) -> NiaoResult<String> {
    let mut out = String::with_capacity(estimate_toml_len(v));
    match v {
        Value::Object(map) => emit_table_rows(map, &[], &mut out, pretty, span)?,
        Value::Array(items)
            if !items.is_empty() && items.iter().all(|s| is_scalar_value(&s.borrow())) =>
        {
            append_inline_array(v, &mut out, span)?;
            out.push('\n');
        }
        other => {
            return Err(RuntimeError::at(
                span,
                E2842_NTOML_TYPE,
                format!(
                    "ntoml_stringify: top-level value must be object or scalar array, got {}",
                    other.type_name()
                ),
            ));
        }
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ntoml_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntoml_parse", span)?;
    let text = string_arg(args, 0, "ntoml_parse", span)?;
    Ok(parse_toml_text(&text, span)?.ref_cell())
}

fn ntoml_parse_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntoml_parse_file", span)?;
    let path = string_arg(args, 0, "ntoml_parse_file", span)?;
    let text = std::fs::read_to_string(&path).map_err(|e| {
        RuntimeError::at(
            span,
            E2841_NTOML_ERROR,
            format!("ntoml_parse_file: cannot read '{path}': {e}"),
        )
    })?;
    Ok(parse_toml_text(&text, span)?.ref_cell())
}

fn ntoml_stringify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntoml_stringify", span)?;
    let out = stringify_toml(&args[0].borrow(), false, span)?;
    Ok(Value::String(out).ref_cell())
}

fn ntoml_stringify_pretty(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntoml_stringify_pretty", span)?;
    let out = stringify_toml(&args[0].borrow(), true, span)?;
    Ok(Value::String(out).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ntoml_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ntoml_fns![
    ("ntoml_parse", "parse", ntoml_parse),
    ("ntoml_parse_file", "parse_file", ntoml_parse_file),
    ("ntoml_stringify", "stringify", ntoml_stringify),
    (
        "ntoml_stringify_pretty",
        "stringify_pretty",
        ntoml_stringify_pretty
    ),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "ntoml";
pub const MODULE_PATHS: &[&str] = &["ntoml", "std/ntoml"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    fn values_equal(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(x), Value::Bool(y)) => x == y,
            (Value::Int(x), Value::Int(y)) => x == y,
            (Value::Float(x), Value::Float(y)) => (x - y).abs() < f64::EPSILON || x == y,
            (Value::String(x), Value::String(y)) => x == y,
            (Value::Array(ax), Value::Array(bx)) => {
                ax.len() == bx.len()
                    && ax
                        .iter()
                        .zip(bx.iter())
                        .all(|(a, b)| values_equal(&a.borrow(), &b.borrow()))
            }
            (Value::Object(ax), Value::Object(bx)) => {
                ax.len() == bx.len()
                    && ax.iter().all(|(k, v)| {
                        bx.get(k)
                            .map(|bv| values_equal(&v.borrow(), &bv.borrow()))
                            .unwrap_or(false)
                    })
            }
            _ => false,
        }
    }

    #[test]
    fn parse_simple() {
        let src = "name = \"niao-demo\"\nversion = \"0.1.0\"\nport = 3001\nenabled = true\n";
        let val = parse_toml_text(src, span()).unwrap();
        match &val {
            Value::Object(map) => {
                assert_eq!(
                    map.get("name").map(|v| v.borrow().clone()),
                    Some(Value::String("niao-demo".into()))
                );
                assert_eq!(
                    map.get("port").map(|v| v.borrow().clone()),
                    Some(Value::Int(3001))
                );
                assert_eq!(
                    map.get("enabled").map(|v| v.borrow().clone()),
                    Some(Value::Bool(true))
                );
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn parse_roundtrip() {
        let src = "[server]\nhost = \"127.0.0.1\"\nport = 3001\n\n[auth]\nmode = \"none\"\n";
        let parsed = parse_toml_text(src, span()).unwrap();
        let emitted = stringify_toml(&parsed, false, span()).unwrap();
        let reparsed = parse_toml_text(&emitted, span()).unwrap();
        assert!(values_equal(&parsed, &reparsed));
    }

    #[test]
    fn array_of_tables_roundtrip() {
        let src = "[[items]]\nname = \"a\"\n[[items]]\nname = \"b\"\n";
        let parsed = parse_toml_text(src, span()).unwrap();
        let emitted = stringify_toml(&parsed, true, span()).unwrap();
        let reparsed = parse_toml_text(&emitted, span()).unwrap();
        assert!(values_equal(&parsed, &reparsed));
    }

    #[test]
    fn inline_array_roundtrip() {
        let src = "tags = [\"a\", \"b\"]\ncounts = [1, 2, 3]\n";
        let parsed = parse_toml_text(src, span()).unwrap();
        let emitted = stringify_toml(&parsed, false, span()).unwrap();
        let reparsed = parse_toml_text(&emitted, span()).unwrap();
        assert!(values_equal(&parsed, &reparsed));
    }
}
