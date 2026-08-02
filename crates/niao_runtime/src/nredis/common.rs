//! Shared argument helpers for nredis builtins.

use crate::{RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;

#[inline]
pub fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> Result<(), RuntimeError> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E2780_NREDIS_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

#[inline]
pub fn arity_range(
    args: &[ValueRef],
    min: usize,
    max: usize,
    name: &str,
    span: Span,
) -> Result<(), RuntimeError> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E2780_NREDIS_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

#[inline]
pub fn handle_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> Result<u64, RuntimeError> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id as u64),
        other => Err(RuntimeError::at(
            span,
            codes::E2783_NREDIS_INVALID_HANDLE,
            format!(
                "{name}() expects Redis handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

#[inline]
pub fn string_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> Result<String, RuntimeError> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(RuntimeError::at(
            span,
            codes::E2782_NREDIS_TYPE,
            format!(
                "{name}() expects string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

#[inline]
pub fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> Result<i64, RuntimeError> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(RuntimeError::at(
            span,
            codes::E2782_NREDIS_TYPE,
            format!(
                "{name}() expects int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

/// Extract an array of strings from `args[idx]`.
pub fn string_array_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> Result<Vec<String>, RuntimeError> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(RuntimeError::at(
                            span,
                            codes::E2782_NREDIS_TYPE,
                            format!(
                                "{name}() array element must be string, got {}",
                                other.type_name()
                            ),
                        ))
                    }
                }
            }
            Ok(out)
        }
        other => Err(RuntimeError::at(
            span,
            codes::E2782_NREDIS_TYPE,
            format!(
                "{name}() expects array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

/// Extract an object (key → string coercion) from `args[idx]`.
/// Int/Float values are stringified automatically.
pub fn object_pairs_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> Result<HashMap<String, String>, RuntimeError> {
    match &*args[idx].borrow() {
        Value::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, v) in map {
                let vs = match &*v.borrow() {
                    Value::String(s) => s.clone(),
                    Value::Int(n) => n.to_string(),
                    Value::Float(f) => f.to_string(),
                    Value::Bool(b) => b.to_string(),
                    other => {
                        return Err(RuntimeError::at(
                            span,
                            codes::E2782_NREDIS_TYPE,
                            format!(
                                "{name}() mset pair value must be string/number, got {}",
                                other.type_name()
                            ),
                        ))
                    }
                };
                out.insert(k.clone(), vs);
            }
            Ok(out)
        }
        other => Err(RuntimeError::at(
            span,
            codes::E2782_NREDIS_TYPE,
            format!(
                "{name}() expects object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}
