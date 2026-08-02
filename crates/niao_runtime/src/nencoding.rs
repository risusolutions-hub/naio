//! Native nencoding standard library — charset detection and transcoding:
//! UTF-8/16, Shift-JIS, GBK, Latin-1, BOM handling (~codecs, charset-normalizer).
//!
//! Import with `import "nencoding"` (or `import "std/nencoding"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_encoding::{
    bom_for, decode, detect, detect_all, encode, is_valid, list_encodings, lookup_encoding,
    normalize, strip_bom, transcode, DecodeErrorMode, EncodeError,
};
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;

const MAX_BYTES: usize = niao_encoding::MAX_BYTES;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E3472_NENCODING_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E3470_NENCODING_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E3470_NENCODING_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nencoding_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3471_NENCODING_ERROR, "nencoding_error", msg.into(), span)
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

fn optional_string(args: &[ValueRef], idx: usize) -> Option<String> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn optional_int(args: &[ValueRef], idx: usize, default: i64) -> i64 {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Int(n) => *n,
        _ => default,
    }
}

fn optional_bool(args: &[ValueRef], idx: usize, default: bool) -> bool {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        _ => default,
    }
}

fn decode_mode_arg(args: &[ValueRef], idx: usize, span: Span) -> Result<DecodeErrorMode, ValueRef> {
    let mode = optional_string(args, idx).unwrap_or_else(|| "strict".into());
    DecodeErrorMode::parse(&mode).ok_or_else(|| {
        nencoding_err(
            span,
            format!("unknown errors mode '{mode}' (use strict, replace, or ignore)"),
        )
    })
}

fn bytes_val(b: Vec<u8>) -> NiaoResult<ValueRef> {
    Ok(Value::ByteArray(b).ref_cell())
}

fn str_val(s: String) -> NiaoResult<ValueRef> {
    Ok(Value::String(s).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn map_err(span: Span, err: EncodeError) -> ValueRef {
    nencoding_err(span, err.message())
}

fn detection_to_object(r: &niao_encoding::DetectionResult) -> HashMap<String, ValueRef> {
    let mut map = HashMap::new();
    map.insert("encoding".into(), Value::String(r.encoding.clone()).ref_cell());
    map.insert("confidence".into(), Value::Float(r.confidence).ref_cell());
    if let Some(ref bom) = r.bom_encoding {
        map.insert("bom_encoding".into(), Value::String(bom.clone()).ref_cell());
    }
    if let Some(ref lang) = r.language {
        map.insert("language".into(), Value::String(lang.clone()).ref_cell());
    }
    map
}

fn encoding_info_to_object(info: &niao_encoding::EncodingInfo) -> HashMap<String, ValueRef> {
    let mut map = HashMap::new();
    map.insert("name".into(), Value::String(info.name.clone()).ref_cell());
    let aliases: Vec<ValueRef> = info
        .aliases
        .iter()
        .map(|a| Value::String(a.clone()).ref_cell())
        .collect();
    map.insert("aliases".into(), Value::Array(aliases).ref_cell());
    map.insert("has_bom".into(), Value::Bool(info.has_bom).ref_cell());
    map
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> nencoding.detect("hello")
// => {encoding: "utf-8", confidence: 1.0}
fn nencoding_detect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nencoding_detect", span)?;
    let bytes = bytes_arg(args, 0, "nencoding_detect", span)?;
    if bytes.len() > MAX_BYTES {
        return Ok(nencoding_err(
            span,
            format!("input size {} exceeds limit {MAX_BYTES}", bytes.len()),
        ));
    }
    let r = detect(&bytes);
    Ok(Value::Object(detection_to_object(&r)).ref_cell())
}

// >>> len(nencoding.detect_all(bytes, 3))
// => 3
fn nencoding_detect_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nencoding_detect_all", span)?;
    let bytes = bytes_arg(args, 0, "nencoding_detect_all", span)?;
    if bytes.len() > MAX_BYTES {
        return Ok(nencoding_err(
            span,
            format!("input size {} exceeds limit {MAX_BYTES}", bytes.len()),
        ));
    }
    let top = optional_int(args, 1, 5) as usize;
    let results = detect_all(&bytes, top);
    let items: Vec<ValueRef> = results
        .iter()
        .map(|r| Value::Object(detection_to_object(r)).ref_cell())
        .collect();
    Ok(Value::Array(items).ref_cell())
}

// >>> nencoding.decode(bytes, "utf-8")
// => "hello"
fn nencoding_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nencoding_decode", span)?;
    let bytes = bytes_arg(args, 0, "nencoding_decode", span)?;
    let encoding = optional_string(args, 1);
    let mode = match decode_mode_arg(args, 2, span) {
        Ok(m) => m,
        Err(v) => return Ok(v),
    };
    match decode(&bytes, encoding.as_deref(), mode) {
        Ok(s) => str_val(s),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(nencoding.encode("hi", "utf-8"))
// => 2
fn nencoding_encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nencoding_encode", span)?;
    let text = string_arg(args, 0, "nencoding_encode", span)?;
    let encoding = optional_string(args, 1).unwrap_or_else(|| "utf-8".into());
    let with_bom = optional_bool(args, 2, false);
    match encode(&text, &encoding, with_bom) {
        Ok(b) => bytes_val(b),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nencoding.transcode(bytes, "utf-8", "shift_jis")
// => utf-8 bytes
fn nencoding_transcode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nencoding_transcode", span)?;
    let bytes = bytes_arg(args, 0, "nencoding_transcode", span)?;
    let to = string_arg(args, 1, "nencoding_transcode", span)?;
    let from = optional_string(args, 2);
    let mode = match decode_mode_arg(args, 3, span) {
        Ok(m) => m,
        Err(v) => return Ok(v),
    };
    match transcode(&bytes, from.as_deref(), &to, mode) {
        Ok(b) => bytes_val(b),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(nencoding.bom("utf-8"))
// => 3
fn nencoding_bom(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nencoding_bom", span)?;
    let encoding = string_arg(args, 0, "nencoding_bom", span)?;
    match bom_for(&encoding) {
        Ok(b) => bytes_val(b),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nencoding.strip_bom(bom_bytes).encoding
// => "utf-8"
fn nencoding_strip_bom(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nencoding_strip_bom", span)?;
    let bytes = bytes_arg(args, 0, "nencoding_strip_bom", span)?;
    let (rest, enc) = strip_bom(&bytes);
    let mut map = HashMap::new();
    map.insert("bytes".into(), Value::ByteArray(rest).ref_cell());
    if let Some(e) = enc {
        map.insert("encoding".into(), Value::String(e).ref_cell());
    }
    Ok(Value::Object(map).ref_cell())
}

// >>> len(nencoding.list()) > 10
// => true
fn nencoding_list(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let items: Vec<ValueRef> = list_encodings()
        .into_iter()
        .map(|info| Value::Object(encoding_info_to_object(&info)).ref_cell())
        .collect();
    Ok(Value::Array(items).ref_cell())
}

// >>> nencoding.lookup("sjis").name
// => "shift_jis"
fn nencoding_lookup(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nencoding_lookup", span)?;
    let label = string_arg(args, 0, "nencoding_lookup", span)?;
    match lookup_encoding(&label) {
        Some(info) => Ok(Value::Object(encoding_info_to_object(&info)).ref_cell()),
        None => Ok(nencoding_err(span, format!("unknown encoding: {label}"))),
    }
}

// >>> nencoding.is_valid(bytes, "utf-8")
// => true
fn nencoding_is_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nencoding_is_valid", span)?;
    let bytes = bytes_arg(args, 0, "nencoding_is_valid", span)?;
    let encoding = string_arg(args, 1, "nencoding_is_valid", span)?;
    bool_val(is_valid(&bytes, &encoding))
}

// >>> nencoding.normalize("e\u0301", "NFC")
// => "é"
fn nencoding_normalize(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nencoding_normalize", span)?;
    let text = string_arg(args, 0, "nencoding_normalize", span)?;
    let form = optional_string(args, 1).unwrap_or_else(|| "NFC".into());
    match normalize(&text, &form) {
        Ok(s) => str_val(s),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nencoding.guess_decode(bytes)
// => "decoded text"
fn nencoding_guess_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nencoding_guess_decode", span)?;
    let bytes = bytes_arg(args, 0, "nencoding_guess_decode", span)?;
    let mode = match decode_mode_arg(args, 1, span) {
        Ok(m) => m,
        Err(v) => return Ok(v),
    };
    match decode(&bytes, None, mode) {
        Ok(s) => str_val(s),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nencoding.same_encoding("utf8", "utf-8")
// => true
fn nencoding_same_encoding(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nencoding_same_encoding", span)?;
    let a = string_arg(args, 0, "nencoding_same_encoding", span)?;
    let b = string_arg(args, 1, "nencoding_same_encoding", span)?;
    let enc_a = niao_encoding::resolve_encoding(&a);
    let enc_b = niao_encoding::resolve_encoding(&b);
    bool_val(enc_a.is_some() && enc_a == enc_b)
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! nencoding_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nencoding_fns![
    ("nencoding_detect", "detect", nencoding_detect),
    ("nencoding_detect_all", "detect_all", nencoding_detect_all),
    ("nencoding_decode", "decode", nencoding_decode),
    ("nencoding_encode", "encode", nencoding_encode),
    ("nencoding_transcode", "transcode", nencoding_transcode),
    ("nencoding_bom", "bom", nencoding_bom),
    ("nencoding_strip_bom", "strip_bom", nencoding_strip_bom),
    ("nencoding_list", "list", nencoding_list),
    ("nencoding_lookup", "lookup", nencoding_lookup),
    ("nencoding_is_valid", "is_valid", nencoding_is_valid),
    ("nencoding_normalize", "normalize", nencoding_normalize),
    ("nencoding_guess_decode", "guess_decode", nencoding_guess_decode),
    ("nencoding_same_encoding", "same_encoding", nencoding_same_encoding),
];

pub const MODULE_NAME: &str = "nencoding";
pub const MODULE_PATHS: &[&str] = &["nencoding", "std/nencoding"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn detect_ascii() {
        let args = vec![Value::String("hello".into()).ref_cell()];
        let out = nencoding_detect(&args, span()).unwrap();
        match &*out.borrow() {
            Value::Object(m) => {
                assert_eq!(
                    match &*m["encoding"].borrow() {
                        Value::String(s) => s.as_str(),
                        _ => panic!(),
                    },
                    "utf-8"
                );
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn encode_utf8() {
        let args = vec![
            Value::String("hi".into()).ref_cell(),
            Value::String("utf-8".into()).ref_cell(),
        ];
        let out = nencoding_encode(&args, span()).unwrap();
        match &*out.borrow() {
            Value::ByteArray(b) => assert_eq!(b, b"hi"),
            other => panic!("expected bytes, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_shift_jis() {
        let text = "日本語";
        let enc_args = vec![
            Value::String(text.into()).ref_cell(),
            Value::String("shift_jis".into()).ref_cell(),
        ];
        let bytes = nencoding_encode(&enc_args, span()).unwrap();
        let dec_args = vec![bytes, Value::String("shift_jis".into()).ref_cell()];
        let out = nencoding_decode(&dec_args, span()).unwrap();
        match &*out.borrow() {
            Value::String(s) => assert_eq!(s, text),
            other => panic!("expected string, got {other:?}"),
        }
    }
}
