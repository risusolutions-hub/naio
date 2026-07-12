//! Native archive standard library — gzip/deflate helpers.
//!
//! Import with `import "archive"` (or `import "std/archive"`).

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
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

fn bytes_result(bytes: Vec<u8>) -> ValueRef {
    match String::from_utf8(bytes) {
        Ok(s) => Value::String(s).ref_cell(),
        Err(e) => Value::ByteArray(e.into_bytes()).ref_cell(),
    }
}

fn archive_err(span: Span, msg: impl Into<String>) -> ValueRef {
    crate::error_value(codes::E1031_CODEC_ERROR, "archive_error", msg.into(), span)
}

fn archive_gzip_encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "archive_gzip_encode", span)?;
    let data = bytes_arg(args, 0, "archive_gzip_encode", span)?;
    match niao_archive::gzip_encode(&data) {
        Ok(out) => Ok(bytes_result(out)),
        Err(e) => Ok(archive_err(span, e.to_string())),
    }
}

fn archive_gzip_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "archive_gzip_decode", span)?;
    let data = bytes_arg(args, 0, "archive_gzip_decode", span)?;
    match niao_archive::gzip_decode(&data) {
        Ok(out) => Ok(bytes_result(out)),
        Err(e) => Ok(archive_err(span, e.to_string())),
    }
}

fn archive_deflate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "archive_deflate", span)?;
    let data = bytes_arg(args, 0, "archive_deflate", span)?;
    match niao_archive::deflate(&data) {
        Ok(out) => Ok(bytes_result(out)),
        Err(e) => Ok(archive_err(span, e.to_string())),
    }
}

fn archive_inflate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "archive_inflate", span)?;
    let data = bytes_arg(args, 0, "archive_inflate", span)?;
    match niao_archive::inflate(&data) {
        Ok(out) => Ok(bytes_result(out)),
        Err(e) => Ok(archive_err(span, e.to_string())),
    }
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    let bind = |map: &mut HashMap<String, ValueRef>, name: &str, f: NativeFn| {
        map.insert(name.to_string(), Value::NativeFunction(f).ref_cell());
    };
    bind(&mut map, "gzip_encode", Rc::new(archive_gzip_encode));
    bind(&mut map, "gzip_decode", Rc::new(archive_gzip_decode));
    bind(&mut map, "deflate", Rc::new(archive_deflate));
    bind(&mut map, "inflate", Rc::new(archive_inflate));
    Value::Object(map)
}

pub const MODULE_NAME: &str = "archive";
pub const MODULE_PATHS: &[&str] = &["archive", "std/archive"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    vec![
        ("archive_gzip_encode", Rc::new(archive_gzip_encode)),
        ("archive_gzip_decode", Rc::new(archive_gzip_decode)),
        ("archive_deflate", Rc::new(archive_deflate)),
        ("archive_inflate", Rc::new(archive_inflate)),
    ]
}
