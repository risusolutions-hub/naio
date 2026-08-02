//! Native ncbor standard library — CBOR encode/decode (IoT / COSE friendly).
//! ~cbor2 subset.
//!
//! Import with `import "ncbor"` (or `import "std/ncbor"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_bignum::BigInt;
use niao_cbor::{
    decode, decode_all, encode_canonical, encode_with_opts, is_valid, tagged, CborError,
    CborValue, DecodeOptions, EncodeOptions, KNOWN, MAX_BYTES, NIAO_SIMPLE_KEY, NIAO_TAG_KEY,
    NIAO_UNDEFINED_KEY, NIAO_VALUE_KEY,
};
use niao_errors::codes;
use std::collections::HashMap;
use std::fs;
use std::rc::Rc;

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E3548_NCBOR_TYPE, msg.into())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E3546_NCBOR_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn ncbor_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3547_NCBOR_ERROR, "ncbor_error", msg.into(), span)
}

fn ncbor_parse_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3549_NCBOR_PARSE, "ncbor_error", msg.into(), span)
}

fn map_err(span: Span, err: CborError) -> ValueRef {
    let code = match &err {
        CborError::Decode(_) | CborError::TrailingData { .. } | CborError::DuplicateKey(_) => {
            codes::E3549_NCBOR_PARSE
        }
        _ => codes::E3547_NCBOR_ERROR,
    };
    error_value(code, "ncbor_error", err.message(), span)
}

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects byte[] or string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
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

fn decode_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> DecodeOptions {
    DecodeOptions {
        max_bytes: int_field(map, "max_bytes", MAX_BYTES as i64).max(1) as usize,
        max_depth: int_field(map, "max_depth", niao_cbor::DEFAULT_MAX_DEPTH as i64).max(1) as usize,
        max_items: int_field(map, "max_items", niao_cbor::DEFAULT_MAX_ITEMS as i64).max(1) as usize,
        tag_hook: bool_field(map, "tag_hook", true),
        allow_indefinite: bool_field(map, "allow_indefinite", true),
        reject_trailing: bool_field(map, "reject_trailing", false),
        reject_duplicate_keys: bool_field(map, "reject_duplicate_keys", false),
    }
}

fn encode_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> EncodeOptions {
    EncodeOptions {
        max_bytes: int_field(map, "max_bytes", MAX_BYTES as i64).max(1) as usize,
        max_depth: int_field(map, "max_depth", niao_cbor::DEFAULT_MAX_DEPTH as i64).max(1) as usize,
        canonical: bool_field(map, "canonical", false),
        sort_keys: bool_field(map, "sort_keys", false),
        auto_datetime_tag: bool_field(map, "auto_datetime_tag", false),
        datetime_timestamp: bool_field(map, "datetime_timestamp", false),
        indefinite_length: bool_field(map, "indefinite_length", false),
        fractional_floats: bool_field(map, "fractional_floats", false),
        self_describe: bool_field(map, "self_describe", false),
    }
}

// ---------------------------------------------------------------------------
// CborValue ↔ Niao Value bridge
// ---------------------------------------------------------------------------

fn cbor_to_niao(c: CborValue) -> Value {
    match c {
        CborValue::Null => Value::Nil,
        CborValue::Undefined => {
            let mut m = HashMap::new();
            m.insert(NIAO_UNDEFINED_KEY.into(), Value::Bool(true).ref_cell());
            Value::Object(m)
        }
        CborValue::Bool(b) => Value::Bool(b),
        CborValue::Int(n) => {
            if n >= i64::MIN as i128 && n <= i64::MAX as i128 {
                Value::Int(n as i64)
            } else if let Ok(u) = u64::try_from(n) {
                Value::BigInt(BigInt::from(u))
            } else {
                Value::BigInt(BigInt::from_i128(n))
            }
        }
        CborValue::BigInt(n) => Value::BigInt(n),
        CborValue::Float(f) => {
            if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                Value::Int(f as i64)
            } else {
                Value::Float(f)
            }
        }
        CborValue::Bytes(b) => Value::ByteArray(b),
        CborValue::String(s) => Value::String(s),
        CborValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(cbor_to_niao(item).ref_cell());
            }
            Value::Array(out)
        }
        CborValue::Map(pairs) => {
            let mut out = HashMap::with_capacity(pairs.len());
            for (k, v) in pairs {
                let key = cbor_key_to_string(&k);
                out.insert(key, cbor_to_niao(v).ref_cell());
            }
            Value::Object(out)
        }
        CborValue::Tag(tag, value) => {
            let mut out = HashMap::new();
            out.insert(NIAO_TAG_KEY.into(), Value::Int(tag as i64).ref_cell());
            out.insert(NIAO_VALUE_KEY.into(), cbor_to_niao(*value).ref_cell());
            Value::Object(out)
        }
        CborValue::Simple(n) => {
            let mut out = HashMap::new();
            out.insert(NIAO_SIMPLE_KEY.into(), Value::Int(n as i64).ref_cell());
            Value::Object(out)
        }
    }
}

fn cbor_key_to_string(k: &CborValue) -> String {
    match k {
        CborValue::String(s) => s.clone(),
        CborValue::Int(n) => n.to_string(),
        CborValue::Bool(b) => b.to_string(),
        CborValue::Float(f) => f.to_string(),
        CborValue::Null => "null".into(),
        CborValue::Bytes(b) => format!("h'{}'", hex_encode(b)),
        other => format!("{other:?}"),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

fn niao_to_cbor(v: &Value, span: Span) -> NiaoResult<CborValue> {
    match v {
        Value::Nil => Ok(CborValue::Null),
        Value::Bool(b) => Ok(CborValue::Bool(*b)),
        Value::Int(n) => Ok(CborValue::Int(*n as i128)),
        Value::BigInt(n) => {
            if let Some(i) = n.to_i64() {
                Ok(CborValue::Int(i as i128))
            } else {
                Ok(CborValue::BigInt(n.clone()))
            }
        }
        Value::Float(f) => Ok(CborValue::Float(*f)),
        Value::String(s) => Ok(CborValue::String(s.clone())),
        Value::ByteArray(b) => Ok(CborValue::Bytes(b.clone())),
        Value::IntArray(items) => Ok(CborValue::Array(
            items.iter().map(|&n| CborValue::Int(n as i128)).collect(),
        )),
        Value::FloatArray(items) => Ok(CborValue::Array(
            items.iter().map(|&f| CborValue::Float(f)).collect(),
        )),
        Value::BoolArray(items) => Ok(CborValue::Array(
            items.iter().map(|&b| CborValue::Bool(b != 0)).collect(),
        )),
        Value::StringArray(items) => {
            let mut seq = Vec::with_capacity(items.len());
            for i in 0..items.len() {
                seq.push(CborValue::String(items.get(i).unwrap_or_default()));
            }
            Ok(CborValue::Array(seq))
        }
        Value::Array(items) => {
            let mut seq = Vec::with_capacity(items.len());
            for slot in items {
                seq.push(niao_to_cbor(&slot.borrow(), span)?);
            }
            Ok(CborValue::Array(seq))
        }
        Value::Object(map) => {
            if map.len() == 1 && map.contains_key(NIAO_UNDEFINED_KEY) {
                return Ok(CborValue::Undefined);
            }
            if let Some(simple) = map.get(NIAO_SIMPLE_KEY) {
                if let Value::Int(n) = &*simple.borrow() {
                    if (0..=255).contains(n) {
                        return Ok(CborValue::Simple(*n as u8));
                    }
                }
            }
            if let (Some(tag_v), Some(val_v)) = (map.get(NIAO_TAG_KEY), map.get(NIAO_VALUE_KEY)) {
                let tag = match &*tag_v.borrow() {
                    Value::Int(n) if *n >= 0 => *n as u64,
                    _ => {
                        return Err(type_err(span, "ncbor: __tag must be a non-negative int"));
                    }
                };
                let inner = niao_to_cbor(&val_v.borrow(), span)?;
                return Ok(tagged(tag, inner));
            }
            let mut pairs = Vec::with_capacity(map.len());
            for (k, v) in map {
                pairs.push((CborValue::String(k.clone()), niao_to_cbor(&v.borrow(), span)?));
            }
            Ok(CborValue::Map(pairs))
        }
        other => Err(type_err(
            span,
            format!("ncbor: cannot encode value of type {}", other.type_name()),
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> ncbor.decode(byte_array[0xA1, 0x61, 0x78, 0x01])
// => {x: 1}
fn ncbor_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncbor_decode", span)?;
    let data = bytes_arg(args, 0, "ncbor_decode", span)?;
    let opts = decode_opts_from_map(optional_object_arg(args, 1).as_ref());
    match decode(&data, &opts) {
        Ok(v) => Ok(cbor_to_niao(v).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> ncbor.encode({x: 1})
// => byte[]
fn ncbor_encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncbor_encode", span)?;
    let cbor = niao_to_cbor(&args[0].borrow(), span)?;
    let opts = encode_opts_from_map(optional_object_arg(args, 1).as_ref());
    match encode_with_opts(&cbor, &opts) {
        Ok(b) => Ok(Value::ByteArray(b).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> ncbor.loads(byte_array[0xA1, 0x61, 0x78, 0x01])
// => {x: 1}
fn ncbor_loads(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    ncbor_decode(args, span)
}

// >>> ncbor.dumps({y: 2})
// => byte[]
fn ncbor_dumps(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    ncbor_encode(args, span)
}

// >>> ncbor.valid(byte_array[0xF4])
// => true
fn ncbor_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ncbor_valid", span)?;
    let data = bytes_arg(args, 0, "ncbor_valid", span)?;
    Ok(Value::Bool(is_valid(&data)).ref_cell())
}

// >>> len(ncbor.decode_all(concat(ncbor.dumps(1), ncbor.dumps(2))))
// => 2
fn ncbor_decode_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncbor_decode_all", span)?;
    let data = bytes_arg(args, 0, "ncbor_decode_all", span)?;
    let opts = decode_opts_from_map(optional_object_arg(args, 1).as_ref());
    match decode_all(&data, &opts) {
        Ok(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(cbor_to_niao(item).ref_cell());
            }
            Ok(Value::Array(out).ref_cell())
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> ncbor.encode_canonical({b: 1, a: 2})
// => byte[]
fn ncbor_encode_canonical(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ncbor_encode_canonical", span)?;
    let cbor = niao_to_cbor(&args[0].borrow(), span)?;
    match encode_canonical(&cbor) {
        Ok(b) => Ok(Value::ByteArray(b).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> ncbor.tag(0, "2020-01-01T00:00:00Z")
// => {__tag: 0, value: "2020-01-01T00:00:00Z"}
fn ncbor_tag(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "ncbor_tag", span)?;
    let tag = match &*args[0].borrow() {
        Value::Int(n) if *n >= 0 => *n as u64,
        other => {
            return Err(type_err(
                span,
                format!("ncbor_tag() expects non-negative int tag, got {}", other.type_name()),
            ));
        }
    };
    let inner = niao_to_cbor(&args[1].borrow(), span)?;
    Ok(cbor_to_niao(tagged(tag, inner)).ref_cell())
}

// >>> ncbor.decode_file("data.cbor")
fn ncbor_decode_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncbor_decode_file", span)?;
    let path = string_arg(args, 0, "ncbor_decode_file", span)?;
    let data = fs::read(&path).map_err(|e| {
        RuntimeError::at(
            span,
            codes::E3547_NCBOR_ERROR,
            format!("ncbor_decode_file: cannot read '{path}': {e}"),
        )
    })?;
    let mut file_args = vec![Value::ByteArray(data).ref_cell()];
    if args.len() > 1 {
        file_args.push(args[1].clone());
    }
    ncbor_decode(&file_args, span)
}

// >>> ncbor.encode_file("out.cbor", {ok: true})
fn ncbor_encode_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncbor_encode_file", span)?;
    let path = string_arg(args, 0, "ncbor_encode_file", span)?;
    let mut enc_args = vec![args[1].clone()];
    if args.len() > 2 {
        enc_args.push(args[2].clone());
    }
    let out = ncbor_encode(&enc_args, span)?;
    let bytes = match out.borrow().clone() {
        Value::ByteArray(b) => b,
        Value::Error { .. } => return Ok(out),
        other => {
            return Err(type_err(
                span,
                format!("ncbor_encode_file: internal error, got {}", other.type_name()),
            ));
        }
    };
    fs::write(&path, &bytes).map_err(|e| {
        RuntimeError::at(
            span,
            codes::E3547_NCBOR_ERROR,
            format!("ncbor_encode_file: cannot write '{path}': {e}"),
        )
    })?;
    Ok(Value::Bool(true).ref_cell())
}

fn tags_namespace() -> Value {
    let mut map = HashMap::new();
    for (name, num) in KNOWN {
        map.insert((*name).to_string(), Value::Int(*num as i64).ref_cell());
    }
    Value::Object(map)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncbor_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncbor_fns![
    ("ncbor_encode", "encode", ncbor_encode),
    ("ncbor_decode", "decode", ncbor_decode),
    ("ncbor_loads", "loads", ncbor_loads),
    ("ncbor_dumps", "dumps", ncbor_dumps),
    ("ncbor_valid", "valid", ncbor_valid),
    ("ncbor_decode_all", "decode_all", ncbor_decode_all),
    ("ncbor_encode_canonical", "encode_canonical", ncbor_encode_canonical),
    ("ncbor_tag", "tag", ncbor_tag),
    ("ncbor_decode_file", "decode_file", ncbor_decode_file),
    ("ncbor_encode_file", "encode_file", ncbor_encode_file),
];

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    map.insert("tags".into(), tags_namespace().ref_cell());
    Value::Object(map)
}

pub const MODULE_NAME: &str = "ncbor";
pub const MODULE_PATHS: &[&str] = &["ncbor", "std/ncbor"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn roundtrip_map() {
        let mut m = HashMap::new();
        m.insert("x".into(), Value::Int(42).ref_cell());
        let args = [Value::Object(m).ref_cell()];
        let enc = ncbor_encode(&args, span()).unwrap();
        let dec_args = [enc];
        let out = ncbor_decode(&dec_args, span()).unwrap();
        match &*out.borrow() {
            Value::Object(o) => assert_eq!(o.get("x").map(|v| v.borrow().clone()), Some(Value::Int(42))),
            other => panic!("expected object, got {other:?}"),
        }
    }
}
