//! Native nnpu standard library — best-effort NPU (neural accelerator)
//! detection and budget mirror. Detects Apple Neural Engine, Intel AI Boost,
//! Qualcomm Hexagon, AMD Ryzen AI, and Linux `/dev/accel` devices. When no
//! NPU exists it says so honestly (`available() == false`) so programs can
//! fall back to GPU or CPU (see `ndevice.best_device()`).
//!
//! Import with `import "nnpu"` (or `import "std/nnpu"`).

use crate::hw;
use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::Ordering;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
            codes::E2730_NNPU_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nnpu_available(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nnpu_available", span)?;
    Ok(Value::Bool(hw::npu_detect().present).ref_cell())
}

fn nnpu_vendor(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nnpu_vendor", span)?;
    Ok(Value::String(hw::npu_detect().vendor).ref_cell())
}

fn nnpu_name(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nnpu_name", span)?;
    Ok(Value::String(hw::npu_detect().name).ref_cell())
}

fn nnpu_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nnpu_info", span)?;
    let npu = hw::npu_detect();
    let mut map = HashMap::new();
    map.insert("available".to_string(), Value::Bool(npu.present).ref_cell());
    map.insert("vendor".to_string(), Value::String(npu.vendor).ref_cell());
    map.insert("name".to_string(), Value::String(npu.name).ref_cell());
    map.insert("note".to_string(), Value::String(npu.note).ref_cell());
    map.insert(
        "limit_pct".to_string(),
        Value::Int(hw::NPU_LIMIT_PCT.load(Ordering::Relaxed) as i64).ref_cell(),
    );
    Ok(Value::Object(map).ref_cell())
}

/// Advisory NPU budget consulted by ndevice.best_device().
fn nnpu_set_limit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nnpu_set_limit", span)?;
    let pct = int_arg(args, 0, "nnpu_set_limit", span)?;
    if !(1..=100).contains(&pct) {
        return Err(type_err(span, "nnpu_set_limit() expects 1..=100"));
    }
    hw::NPU_LIMIT_PCT.store(pct as u8, Ordering::Relaxed);
    Ok(Value::Nil.ref_cell())
}

fn nnpu_get_limit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nnpu_get_limit", span)?;
    Ok(Value::Int(hw::NPU_LIMIT_PCT.load(Ordering::Relaxed) as i64).ref_cell())
}

/// Safe to schedule NPU work: present and not globally throttled.
fn nnpu_ok(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nnpu_ok", span)?;
    let ok = hw::npu_detect().present && hw::throttle_level() < 2;
    Ok(Value::Bool(ok).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nnpu_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nnpu_fns![
    ("nnpu_available", "available", nnpu_available),
    ("nnpu_vendor", "vendor", nnpu_vendor),
    ("nnpu_name", "name", nnpu_name),
    ("nnpu_info", "info", nnpu_info),
    ("nnpu_set_limit", "set_limit", nnpu_set_limit),
    ("nnpu_get_limit", "get_limit", nnpu_get_limit),
    ("nnpu_ok", "ok", nnpu_ok),
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

pub const MODULE_NAME: &str = "nnpu";
pub const MODULE_PATHS: &[&str] = &["nnpu", "std/nnpu"];

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

    #[test]
    fn info_shape() {
        match &*nnpu_info(&[], span()).unwrap().borrow() {
            Value::Object(map) => {
                assert!(map.contains_key("available"));
                assert!(map.contains_key("vendor"));
                assert!(map.contains_key("note"));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn limit_roundtrip() {
        nnpu_set_limit(&[Value::Int(60).ref_cell()], span()).unwrap();
        match &*nnpu_get_limit(&[], span()).unwrap().borrow() {
            Value::Int(n) => assert_eq!(*n, 60),
            other => panic!("expected int, got {other:?}"),
        }
        nnpu_set_limit(&[Value::Int(100).ref_cell()], span()).unwrap();
    }
}
