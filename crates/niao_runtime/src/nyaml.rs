//! Native nyaml standard library — YAML 1.2 parse + emit, safe-by-default,
//! anchors, multi-doc (~PyYAML, ruamel.yaml subset).
//!
//! Import with `import "nyaml"` (or `import "std/nyaml"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_bignum::BigInt;
use niao_errors::codes;
use niao_yaml::{
    emit, emit_all, emit_pretty, is_valid, parse, parse_all, EmitOptions, ParseOptions, YamlError,
    YamlValue, MAX_BYTES,
};
use std::collections::HashMap;
use std::fs;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4302_NYAML_TYPE, msg.into())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4300_NYAML_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nyaml_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4301_NYAML_ERROR, "nyaml_error", msg.into(), span)
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

fn optional_object_arg(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Some(map.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn bool_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        Some(Value::String(s)) => matches!(s.as_str(), "true" | "1" | "yes" | "on"),
        _ => default,
    }
}

fn int_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: i64) -> i64 {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => n,
        _ => default,
    }
}

fn parse_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> ParseOptions {
    ParseOptions {
        safe: bool_field(map, "safe", true),
        multi: bool_field(map, "multi", false),
    }
}

fn emit_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> EmitOptions {
    let flow = map.and_then(|m| {
        if m.contains_key("flow") {
            Some(bool_field(Some(m), "flow", false))
        } else {
            None
        }
    });
    EmitOptions {
        flow,
        indent: int_field(map, "indent", 2).max(1) as usize,
        width: int_field(map, "width", 80).max(0) as usize,
        sort_keys: bool_field(map, "sort_keys", false),
        explicit_start: bool_field(map, "explicit_start", false),
        explicit_end: bool_field(map, "explicit_end", false),
        unicode: bool_field(map, "unicode", true),
    }
}

fn map_yaml_err(span: Span, err: YamlError) -> ValueRef {
    let code = match &err {
        YamlError::Parse(_) | YamlError::MultiDocSingle | YamlError::UnsafeTag(_) => {
            codes::E4303_NYAML_PARSE
        }
        _ => codes::E4301_NYAML_ERROR,
    };
    error_value(code, "nyaml_error", err.message(), span)
}

// ---------------------------------------------------------------------------
// YamlValue ↔ Niao Value bridge
// ---------------------------------------------------------------------------

fn yaml_to_niao(y: YamlValue) -> Value {
    match y {
        YamlValue::Null => Value::Nil,
        YamlValue::Bool(b) => Value::Bool(b),
        YamlValue::Int(n) => Value::Int(n),
        YamlValue::BigInt(n) => Value::BigInt(n),
        YamlValue::Float(f) => {
            if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                Value::Int(f as i64)
            } else {
                Value::Float(f)
            }
        }
        YamlValue::String(s) => Value::String(s),
        YamlValue::Sequence(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(yaml_to_niao(item).ref_cell());
            }
            Value::Array(out)
        }
        YamlValue::Mapping(pairs) => {
            let mut out = HashMap::with_capacity(pairs.len());
            for (k, v) in pairs {
                let key = yaml_key_to_string(&k);
                out.insert(key, yaml_to_niao(v).ref_cell());
            }
            Value::Object(out)
        }
        YamlValue::Tagged { tag, value } => {
            let mut out = HashMap::new();
            out.insert("__tag".into(), Value::String(tag).ref_cell());
            out.insert("value".into(), yaml_to_niao(*value).ref_cell());
            Value::Object(out)
        }
    }
}

fn yaml_key_to_string(k: &YamlValue) -> String {
    match k {
        YamlValue::String(s) => s.clone(),
        YamlValue::Int(n) => n.to_string(),
        YamlValue::Bool(b) => b.to_string(),
        YamlValue::Float(f) => f.to_string(),
        YamlValue::Null => "null".into(),
        other => format!("{other:?}"),
    }
}

fn niao_to_yaml(v: &Value, span: Span) -> NiaoResult<YamlValue> {
    match v {
        Value::Nil => Ok(YamlValue::Null),
        Value::Bool(b) => Ok(YamlValue::Bool(*b)),
        Value::Int(n) => Ok(YamlValue::Int(*n)),
        Value::BigInt(n) => Ok(YamlValue::BigInt(n.clone())),
        Value::Float(f) => Ok(YamlValue::Float(*f)),
        Value::String(s) => Ok(YamlValue::String(s.clone())),
        Value::IntArray(items) => Ok(YamlValue::Sequence(
            items.iter().map(|&n| YamlValue::Int(n)).collect(),
        )),
        Value::FloatArray(items) => Ok(YamlValue::Sequence(
            items.iter().map(|&f| YamlValue::Float(f)).collect(),
        )),
        Value::BoolArray(items) => Ok(YamlValue::Sequence(
            items.iter().map(|&b| YamlValue::Bool(b != 0)).collect(),
        )),
        Value::ByteArray(items) => Ok(YamlValue::Sequence(
            items.iter().map(|&b| YamlValue::Int(b as i64)).collect(),
        )),
        Value::StringArray(items) => {
            let mut seq = Vec::with_capacity(items.len());
            for i in 0..items.len() {
                seq.push(YamlValue::String(items.get(i).unwrap_or_default()));
            }
            Ok(YamlValue::Sequence(seq))
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for slot in items {
                out.push(niao_to_yaml(&slot.borrow(), span)?);
            }
            Ok(YamlValue::Sequence(out))
        }
        Value::Object(map) => {
            if let (Some(tag_v), Some(val_v)) = (map.get("__tag"), map.get("value")) {
                if let Value::String(tag) = &*tag_v.borrow() {
                    return Ok(YamlValue::Tagged {
                        tag: tag.clone(),
                        value: Box::new(niao_to_yaml(&val_v.borrow(), span)?),
                    });
                }
            }
            let mut pairs = Vec::with_capacity(map.len());
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for k in keys {
                if k == "__tag" {
                    continue;
                }
                pairs.push((
                    YamlValue::String(k.clone()),
                    niao_to_yaml(&map[k].borrow(), span)?,
                ));
            }
            Ok(YamlValue::Mapping(pairs))
        }
        other => Err(type_err(
            span,
            format!("nyaml: cannot encode value of type {}", other.type_name()),
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> nyaml.parse("x: 1")
// => {x: 1}
fn nyaml_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nyaml_parse", span)?;
    let text = string_arg(args, 0, "nyaml_parse", span)?;
    if text.len() > MAX_BYTES {
        return Ok(nyaml_err(
            span,
            format!("input size {} exceeds limit {MAX_BYTES}", text.len()),
        ));
    }
    let opts = parse_opts_from_map(optional_object_arg(args, 1).as_ref());
    match parse(&text, &opts) {
        Ok(v) => Ok(yaml_to_niao(v).ref_cell()),
        Err(e) => Ok(map_yaml_err(span, e)),
    }
}

// >>> len(nyaml.parse_all("---\n{a:1}\n---\n{b:2}\n"))
// => 2
fn nyaml_parse_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nyaml_parse_all", span)?;
    let text = string_arg(args, 0, "nyaml_parse_all", span)?;
    if text.len() > MAX_BYTES {
        return Ok(nyaml_err(
            span,
            format!("input size {} exceeds limit {MAX_BYTES}", text.len()),
        ));
    }
    let mut opts = parse_opts_from_map(optional_object_arg(args, 1).as_ref());
    opts.multi = true;
    match parse_all(&text, &opts) {
        Ok(docs) => {
            let items: Vec<ValueRef> = docs.into_iter().map(|d| yaml_to_niao(d).ref_cell()).collect();
            Ok(Value::Array(items).ref_cell())
        }
        Err(e) => Ok(map_yaml_err(span, e)),
    }
}

// >>> nyaml.safe_parse("key: ok")
// => {key: "ok"}
fn nyaml_safe_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nyaml_safe_parse", span)?;
    let text = string_arg(args, 0, "nyaml_safe_parse", span)?;
    match parse(&text, &ParseOptions::default()) {
        Ok(v) => Ok(yaml_to_niao(v).ref_cell()),
        Err(e) => Ok(map_yaml_err(span, e)),
    }
}

// >>> len(nyaml.safe_parse_all("---\n1\n---\n2\n"))
// => 2
fn nyaml_safe_parse_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nyaml_safe_parse_all", span)?;
    let text = string_arg(args, 0, "nyaml_safe_parse_all", span)?;
    let opts = ParseOptions {
        safe: true,
        multi: true,
    };
    match parse_all(&text, &opts) {
        Ok(docs) => {
            let items: Vec<ValueRef> = docs.into_iter().map(|d| yaml_to_niao(d).ref_cell()).collect();
            Ok(Value::Array(items).ref_cell())
        }
        Err(e) => Ok(map_yaml_err(span, e)),
    }
}

// >>> nyaml.parse_file("config.yaml").name
// => "demo"
fn nyaml_parse_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nyaml_parse_file", span)?;
    let path = string_arg(args, 0, "nyaml_parse_file", span)?;
    let text = fs::read_to_string(&path).map_err(|e| {
        RuntimeError::at(
            span,
            codes::E4301_NYAML_ERROR,
            format!("nyaml_parse_file: cannot read '{path}': {e}"),
        )
    })?;
    let opts = parse_opts_from_map(optional_object_arg(args, 1).as_ref());
    match parse(&text, &opts) {
        Ok(v) => Ok(yaml_to_niao(v).ref_cell()),
        Err(e) => Ok(map_yaml_err(span, e)),
    }
}

// >>> nyaml.emit({a: 1, b: [2, 3]})
// => "a: 1\nb:\n- 2\n- 3\n"
fn nyaml_emit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nyaml_emit", span)?;
    let yv = niao_to_yaml(&args[0].borrow(), span)?;
    let opts = emit_opts_from_map(optional_object_arg(args, 1).as_ref());
    match emit(&yv, &opts) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_yaml_err(span, e)),
    }
}

// >>> nyaml.emit_pretty({x: 1})
// => "x: 1\n"
fn nyaml_emit_pretty(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nyaml_emit_pretty", span)?;
    let yv = niao_to_yaml(&args[0].borrow(), span)?;
    let indent = int_field(optional_object_arg(args, 1).as_ref(), "indent", 2).max(1) as usize;
    match emit_pretty(&yv, indent) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_yaml_err(span, e)),
    }
}

// >>> nyaml.emit_all([{a: 1}, {b: 2}])
// => "---\na: 1\n---\nb: 2\n"
fn nyaml_emit_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nyaml_emit_all", span)?;
    let docs = match &*args[0].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for slot in items {
                out.push(niao_to_yaml(&slot.borrow(), span)?);
            }
            out
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "nyaml_emit_all() expects an array as argument 1, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let opts = emit_opts_from_map(optional_object_arg(args, 1).as_ref());
    match emit_all(&docs, &opts) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_yaml_err(span, e)),
    }
}

// >>> nyaml.emit_file("out.yaml", {done: true})
// => true
fn nyaml_emit_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nyaml_emit_file", span)?;
    let path = string_arg(args, 0, "nyaml_emit_file", span)?;
    let yv = niao_to_yaml(&args[1].borrow(), span)?;
    let opts = emit_opts_from_map(optional_object_arg(args, 2).as_ref());
    let text = emit(&yv, &opts).map_err(|e| {
        RuntimeError::at(span, codes::E4301_NYAML_ERROR, e.message())
    })?;
    fs::write(&path, &text).map_err(|e| {
        RuntimeError::at(
            span,
            codes::E4301_NYAML_ERROR,
            format!("nyaml_emit_file: cannot write '{path}': {e}"),
        )
    })?;
    Ok(Value::Bool(true).ref_cell())
}

// >>> nyaml.valid("key: value")
// => true
fn nyaml_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nyaml_valid", span)?;
    let text = string_arg(args, 0, "nyaml_valid", span)?;
    Ok(Value::Bool(is_valid(&text)).ref_cell())
}

// >>> nyaml.load("n: 42")
// => {n: 42}
fn nyaml_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nyaml_parse(args, span)
}

// >>> nyaml.dump({z: 9})
// => "z: 9\n"
fn nyaml_dump(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nyaml_emit(args, span)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nyaml_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nyaml_fns![
    ("nyaml_parse", "parse", nyaml_parse),
    ("nyaml_parse_all", "parse_all", nyaml_parse_all),
    ("nyaml_safe_parse", "safe_parse", nyaml_safe_parse),
    ("nyaml_safe_parse_all", "safe_parse_all", nyaml_safe_parse_all),
    ("nyaml_parse_file", "parse_file", nyaml_parse_file),
    ("nyaml_emit", "emit", nyaml_emit),
    ("nyaml_emit_pretty", "emit_pretty", nyaml_emit_pretty),
    ("nyaml_emit_all", "emit_all", nyaml_emit_all),
    ("nyaml_emit_file", "emit_file", nyaml_emit_file),
    ("nyaml_valid", "valid", nyaml_valid),
    ("nyaml_load", "load", nyaml_load),
    ("nyaml_dump", "dump", nyaml_dump),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nyaml";
pub const MODULE_PATHS: &[&str] = &["nyaml", "std/nyaml"];

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

    fn call_parse(src: &str) -> Value {
        let args = [Value::String(src.to_string()).ref_cell()];
        match nyaml_parse(&args, span()) {
            Ok(v) => v.borrow().clone(),
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn parse_map() {
        let v = call_parse("name: niao\nport: 8080\n");
        match v {
            Value::Object(m) => {
                assert_eq!(
                    m.get("name").map(|v| v.borrow().clone()),
                    Some(Value::String("niao".into()))
                );
                assert_eq!(
                    m.get("port").map(|v| v.borrow().clone()),
                    Some(Value::Int(8080))
                );
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_emit() {
        let parsed = call_parse("items:\n  - a\n  - b\n");
        let args = [parsed.ref_cell()];
        let out = nyaml_emit(&args, span()).unwrap();
        match &*out.borrow() {
            Value::String(s) => {
                let reparsed = call_parse(s);
                assert!(matches!(reparsed, Value::Object(_)));
            }
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn safe_rejects_python_object() {
        let src = "!!python/object/apply:os.system\n- echo\n";
        let args = [Value::String(src.to_string()).ref_cell()];
        let out = nyaml_parse(&args, span()).unwrap();
        match &*out.borrow() {
            Value::Error(_) => {}
            other => panic!("expected error, got {other:?}"),
        }
    }
}
