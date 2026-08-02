//! Native codec standard library — base64, hex, UUID.
//!
//! Import with `import "codec"` (or `import "std/codec"`).

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_codec::{
    base64::{self, Alphabet, Base64Config},
    hex,
    uuid::Uuid,
};
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;

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
            codes::E1030_CODEC_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(
    args: &[ValueRef],
    min: usize,
    max: usize,
    name: &str,
    span: Span,
) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E1030_CODEC_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
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

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::ByteArray(b) => Ok(b.iter().map(|&x| x as u8).collect()),
        Value::IntArray(b) => Ok(b.iter().map(|&x| x as u8).collect()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string or bytes as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_bool_arg(args: &[ValueRef], idx: usize, default: bool) -> bool {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        _ => default,
    }
}

fn optional_string_arg(args: &[ValueRef], idx: usize) -> Option<String> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn b64_config(args: &[ValueRef], start: usize) -> Base64Config {
    let mut config = Base64Config::STANDARD;
    if let Some(mode) = optional_string_arg(args, start) {
        match mode.as_str() {
            "url" | "url_safe" => config.alphabet = Alphabet::UrlSafe,
            _ => config.alphabet = Alphabet::Standard,
        }
    }
    if args.len() > start + 1 {
        config.padding = optional_bool_arg(args, start + 1, true);
    }
    config
}

fn codec_err(span: Span, msg: impl Into<String>) -> ValueRef {
    crate::error_value(codes::E1031_CODEC_ERROR, "codec_error", msg.into(), span)
}

fn codec_b64encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "codec_b64encode", span)?;
    let data = bytes_arg(args, 0, "codec_b64encode", span)?;
    let config = b64_config(args, 1);
    Ok(Value::String(base64::encode(&data, config)).ref_cell())
}

fn codec_b64decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "codec_b64decode", span)?;
    let input = string_arg(args, 0, "codec_b64decode", span)?;
    let config = b64_config(args, 1);
    match base64::decode(&input, config) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(s) => Ok(Value::String(s).ref_cell()),
            Err(e) => Ok(Value::ByteArray(e.into_bytes()).ref_cell()),
        },
        Err(e) => Ok(codec_err(span, e.to_string())),
    }
}

fn codec_hexencode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "codec_hexencode", span)?;
    let data = bytes_arg(args, 0, "codec_hexencode", span)?;
    let upper = optional_bool_arg(args, 1, false);
    Ok(Value::String(hex::encode_with(&data, upper)).ref_cell())
}

fn codec_hexdecode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "codec_hexdecode", span)?;
    let input = string_arg(args, 0, "codec_hexdecode", span)?;
    match hex::decode(&input) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(s) => Ok(Value::String(s).ref_cell()),
            Err(e) => Ok(Value::ByteArray(e.into_bytes()).ref_cell()),
        },
        Err(e) => Ok(codec_err(span, e.to_string())),
    }
}

fn codec_uuid4(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "codec_uuid4", span)?;
    Ok(Value::String(Uuid::new_v4().to_string()).ref_cell())
}

fn codec_uuid7(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "codec_uuid7", span)?;
    Ok(Value::String(Uuid::new_v7().to_string()).ref_cell())
}

fn codec_uuid_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "codec_uuid_parse", span)?;
    let s = string_arg(args, 0, "codec_uuid_parse", span)?;
    match Uuid::parse(&s) {
        Ok(u) => Ok(Value::String(u.to_string()).ref_cell()),
        Err(e) => Ok(codec_err(span, e.to_string())),
    }
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    let bind = |map: &mut HashMap<String, ValueRef>, name: &str, f: NativeFn| {
        map.insert(name.to_string(), Value::NativeFunction(f).ref_cell());
    };
    bind(&mut map, "b64encode", Rc::new(codec_b64encode));
    bind(&mut map, "b64decode", Rc::new(codec_b64decode));
    bind(&mut map, "hexencode", Rc::new(codec_hexencode));
    bind(&mut map, "hexdecode", Rc::new(codec_hexdecode));
    bind(&mut map, "uuid4", Rc::new(codec_uuid4));
    bind(&mut map, "uuid7", Rc::new(codec_uuid7));
    bind(&mut map, "uuid_parse", Rc::new(codec_uuid_parse));
    Value::Object(map)
}

pub const MODULE_NAME: &str = "codec";
pub const MODULE_PATHS: &[&str] = &["codec", "std/codec"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    vec![
        ("codec_b64encode", Rc::new(codec_b64encode)),
        ("codec_b64decode", Rc::new(codec_b64decode)),
        ("codec_hexencode", Rc::new(codec_hexencode)),
        ("codec_hexdecode", Rc::new(codec_hexdecode)),
        ("codec_uuid4", Rc::new(codec_uuid4)),
        ("codec_uuid7", Rc::new(codec_uuid7)),
        ("codec_uuid_parse", Rc::new(codec_uuid_parse)),
    ]
}
