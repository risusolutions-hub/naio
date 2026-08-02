//! Native ncpu standard library — CPU detection, live usage, temperature,
//! and cooperative user limits (`set_limit(40)` = use at most 40% of cores).
//! `threads()` is the number every Niao worker pool should use right now:
//! it honors the user limit *and* the ndevice thermal throttle.
//!
//! Import with `import "ncpu"` (or `import "std/ncpu"`).

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
            codes::E2700_NCPU_ARITY,
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

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

fn opt_num(n: i64) -> ValueRef {
    if n < 0 {
        Value::Nil.ref_cell()
    } else {
        Value::Int(n).ref_cell()
    }
}

fn opt_pct(p: f64) -> ValueRef {
    if p < 0.0 {
        Value::Nil.ref_cell()
    } else {
        Value::Float(p).ref_cell()
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ncpu_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncpu_count", span)?;
    int_val(hw::logical_cores() as i64)
}

fn ncpu_physical_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncpu_physical_count", span)?;
    Ok(opt_num(hw::physical_cores()))
}

fn ncpu_arch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncpu_arch", span)?;
    Ok(Value::String(std::env::consts::ARCH.to_string()).ref_cell())
}

fn ncpu_brand(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncpu_brand", span)?;
    Ok(Value::String(hw::cpu_brand()).ref_cell())
}

/// System-wide CPU usage percent, or nil when the platform can't report it.
fn ncpu_usage(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncpu_usage", span)?;
    Ok(opt_pct(hw::cpu_usage_pct()))
}

/// CPU package temperature in °C, or nil when unavailable.
fn ncpu_temp_c(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncpu_temp_c", span)?;
    Ok(opt_num(hw::cpu_temp_c()))
}

/// Limit Niao's CPU appetite to a percentage of logical cores (1..=100).
fn ncpu_set_limit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncpu_set_limit", span)?;
    let pct = int_arg(args, 0, "ncpu_set_limit", span)?;
    if !(1..=100).contains(&pct) {
        return Err(type_err(span, "ncpu_set_limit() expects 1..=100"));
    }
    hw::CPU_LIMIT_PCT.store(pct as u8, Ordering::Relaxed);
    Ok(Value::Nil.ref_cell())
}

fn ncpu_get_limit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncpu_get_limit", span)?;
    int_val(hw::CPU_LIMIT_PCT.load(Ordering::Relaxed) as i64)
}

/// Worker count to use right now (limit + thermal throttle applied).
fn ncpu_threads(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncpu_threads", span)?;
    int_val(hw::allowed_threads() as i64)
}

/// Optional max-temp for the CPU used by the ndevice guard (0 disables).
fn ncpu_set_max_temp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncpu_set_max_temp", span)?;
    let c = int_arg(args, 0, "ncpu_set_max_temp", span)?;
    if !(0..=110).contains(&c) {
        return Err(type_err(span, "ncpu_set_max_temp() expects 0..=110"));
    }
    hw::CPU_MAX_TEMP_C.store(c as u8, Ordering::Relaxed);
    Ok(Value::Nil.ref_cell())
}

fn ncpu_get_max_temp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncpu_get_max_temp", span)?;
    int_val(hw::CPU_MAX_TEMP_C.load(Ordering::Relaxed) as i64)
}

fn ncpu_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncpu_info", span)?;
    let mut map = HashMap::new();
    map.insert(
        "cores".to_string(),
        Value::Int(hw::logical_cores() as i64).ref_cell(),
    );
    map.insert("physical_cores".to_string(), opt_num(hw::physical_cores()));
    map.insert(
        "arch".to_string(),
        Value::String(std::env::consts::ARCH.to_string()).ref_cell(),
    );
    map.insert(
        "brand".to_string(),
        Value::String(hw::cpu_brand()).ref_cell(),
    );
    map.insert("usage".to_string(), opt_pct(hw::cpu_usage_pct()));
    map.insert("temp_c".to_string(), opt_num(hw::cpu_temp_c()));
    map.insert(
        "limit_pct".to_string(),
        Value::Int(hw::CPU_LIMIT_PCT.load(Ordering::Relaxed) as i64).ref_cell(),
    );
    map.insert(
        "max_temp_c".to_string(),
        Value::Int(hw::CPU_MAX_TEMP_C.load(Ordering::Relaxed) as i64).ref_cell(),
    );
    map.insert(
        "threads".to_string(),
        Value::Int(hw::allowed_threads() as i64).ref_cell(),
    );
    Ok(Value::Object(map).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncpu_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncpu_fns![
    ("ncpu_count", "count", ncpu_count),
    ("ncpu_physical_count", "physical_count", ncpu_physical_count),
    ("ncpu_arch", "arch", ncpu_arch),
    ("ncpu_brand", "brand", ncpu_brand),
    ("ncpu_usage", "usage", ncpu_usage),
    ("ncpu_temp_c", "temp_c", ncpu_temp_c),
    ("ncpu_set_limit", "set_limit", ncpu_set_limit),
    ("ncpu_get_limit", "get_limit", ncpu_get_limit),
    ("ncpu_threads", "threads", ncpu_threads),
    ("ncpu_set_max_temp", "set_max_temp", ncpu_set_max_temp),
    ("ncpu_get_max_temp", "get_max_temp", ncpu_get_max_temp),
    ("ncpu_info", "info", ncpu_info),
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

pub const MODULE_NAME: &str = "ncpu";
pub const MODULE_PATHS: &[&str] = &["ncpu", "std/ncpu"];

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
    fn count_positive() {
        match &*ncpu_count(&[], span()).unwrap().borrow() {
            Value::Int(n) => assert!(*n >= 1),
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[test]
    fn limit_roundtrip_and_threads() {
        ncpu_set_limit(&[Value::Int(50).ref_cell()], span()).unwrap();
        match &*ncpu_get_limit(&[], span()).unwrap().borrow() {
            Value::Int(n) => assert_eq!(*n, 50),
            other => panic!("expected int, got {other:?}"),
        }
        match &*ncpu_threads(&[], span()).unwrap().borrow() {
            Value::Int(n) => assert!(*n >= 1),
            other => panic!("expected int, got {other:?}"),
        }
        ncpu_set_limit(&[Value::Int(100).ref_cell()], span()).unwrap();
    }

    #[test]
    fn bad_limit_rejected() {
        assert!(ncpu_set_limit(&[Value::Int(0).ref_cell()], span()).is_err());
        assert!(ncpu_set_limit(&[Value::Int(101).ref_cell()], span()).is_err());
    }

    #[test]
    fn info_shape() {
        match &*ncpu_info(&[], span()).unwrap().borrow() {
            Value::Object(map) => {
                assert!(map.contains_key("cores"));
                assert!(map.contains_key("threads"));
                assert!(map.contains_key("limit_pct"));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
