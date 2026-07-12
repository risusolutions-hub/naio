//! Native nram standard library — system and process memory readings plus a
//! cooperative RAM budget: `set_limit_mb(2048)` or `set_limit_pct(50)`, then
//! gate big allocations with `ok(extra_mb)` and watch `pressure()`.
//!
//! Import with `import "nram"` (or `import "std/nram"`).

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
            codes::E2720_NRAM_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E2720_NRAM_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
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

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nram_total_mb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nram_total_mb", span)?;
    Ok(opt_num(hw::ram_stats_mb().0))
}

fn nram_available_mb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nram_available_mb", span)?;
    Ok(opt_num(hw::ram_stats_mb().1))
}

fn nram_used_mb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nram_used_mb", span)?;
    let (total, avail) = hw::ram_stats_mb();
    if total < 0 || avail < 0 {
        return Ok(Value::Nil.ref_cell());
    }
    int_val(total - avail)
}

/// System memory usage percent, or nil when unavailable.
fn nram_usage(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nram_usage", span)?;
    let (total, avail) = hw::ram_stats_mb();
    if total <= 0 || avail < 0 {
        return Ok(Value::Nil.ref_cell());
    }
    Ok(Value::Float((total - avail) as f64 * 100.0 / total as f64).ref_cell())
}

/// This process's resident memory in MB.
fn nram_process_mb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nram_process_mb", span)?;
    int_val(hw::process_mb())
}

fn nram_set_limit_mb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nram_set_limit_mb", span)?;
    let mb = int_arg(args, 0, "nram_set_limit_mb", span)?;
    if mb < 0 {
        return Err(type_err(span, "nram_set_limit_mb() expects >= 0 (0 disables)"));
    }
    hw::RAM_LIMIT_MB.store(mb, Ordering::Relaxed);
    Ok(Value::Nil.ref_cell())
}

fn nram_set_limit_pct(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nram_set_limit_pct", span)?;
    let pct = int_arg(args, 0, "nram_set_limit_pct", span)?;
    if !(0..=100).contains(&pct) {
        return Err(type_err(span, "nram_set_limit_pct() expects 0..=100 (0 disables)"));
    }
    hw::RAM_LIMIT_PCT.store(pct as u8, Ordering::Relaxed);
    Ok(Value::Nil.ref_cell())
}

/// Effective budget in MB after combining mb/pct limits (0 = unlimited).
fn nram_get_limit_mb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nram_get_limit_mb", span)?;
    int_val(hw::ram_budget_mb())
}

/// Would `extra_mb` more memory fit inside the budget and system headroom?
fn nram_ok(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nram_ok", span)?;
    let extra = if args.is_empty() {
        0
    } else {
        int_arg(args, 0, "nram_ok", span)?
    };
    if extra < 0 {
        return Err(type_err(span, "nram_ok() expects extra_mb >= 0"));
    }
    Ok(Value::Bool(hw::ram_ok(extra)).ref_cell())
}

/// "low" | "medium" | "high" | "critical" | "unknown".
fn nram_pressure(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nram_pressure", span)?;
    Ok(Value::String(hw::ram_pressure().to_string()).ref_cell())
}

/// MB still usable: min(system available, budget remaining). Nil if unknown.
fn nram_headroom_mb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nram_headroom_mb", span)?;
    let (_, avail) = hw::ram_stats_mb();
    let budget = hw::ram_budget_mb();
    let budget_left = if budget > 0 {
        (budget - hw::process_mb()).max(0)
    } else {
        i64::MAX
    };
    if avail < 0 && budget == 0 {
        return Ok(Value::Nil.ref_cell());
    }
    let sys_left = if avail < 0 { i64::MAX } else { avail };
    int_val(sys_left.min(budget_left))
}

fn nram_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nram_info", span)?;
    let (total, avail) = hw::ram_stats_mb();
    let mut map = HashMap::new();
    map.insert("total_mb".to_string(), opt_num(total));
    map.insert("available_mb".to_string(), opt_num(avail));
    map.insert(
        "used_mb".to_string(),
        if total >= 0 && avail >= 0 {
            Value::Int(total - avail).ref_cell()
        } else {
            Value::Nil.ref_cell()
        },
    );
    map.insert("process_mb".to_string(), Value::Int(hw::process_mb()).ref_cell());
    map.insert("limit_mb".to_string(), Value::Int(hw::ram_budget_mb()).ref_cell());
    map.insert(
        "pressure".to_string(),
        Value::String(hw::ram_pressure().to_string()).ref_cell(),
    );
    Ok(Value::Object(map).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nram_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nram_fns![
    ("nram_total_mb", "total_mb", nram_total_mb),
    ("nram_available_mb", "available_mb", nram_available_mb),
    ("nram_used_mb", "used_mb", nram_used_mb),
    ("nram_usage", "usage", nram_usage),
    ("nram_process_mb", "process_mb", nram_process_mb),
    ("nram_set_limit_mb", "set_limit_mb", nram_set_limit_mb),
    ("nram_set_limit_pct", "set_limit_pct", nram_set_limit_pct),
    ("nram_get_limit_mb", "get_limit_mb", nram_get_limit_mb),
    ("nram_ok", "ok", nram_ok),
    ("nram_pressure", "pressure", nram_pressure),
    ("nram_headroom_mb", "headroom_mb", nram_headroom_mb),
    ("nram_info", "info", nram_info),
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

pub const MODULE_NAME: &str = "nram";
pub const MODULE_PATHS: &[&str] = &["nram", "std/nram"];

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
    fn process_mb_positive() {
        match &*nram_process_mb(&[], span()).unwrap().borrow() {
            Value::Int(n) => assert!(*n >= 0),
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[test]
    fn limit_roundtrip() {
        nram_set_limit_mb(&[Value::Int(4096).ref_cell()], span()).unwrap();
        match &*nram_get_limit_mb(&[], span()).unwrap().borrow() {
            Value::Int(n) => assert_eq!(*n, 4096),
            other => panic!("expected int, got {other:?}"),
        }
        nram_set_limit_mb(&[Value::Int(0).ref_cell()], span()).unwrap();
    }

    #[test]
    fn ok_with_huge_request_fails_under_budget() {
        nram_set_limit_mb(&[Value::Int(1).ref_cell()], span()).unwrap();
        match &*nram_ok(&[Value::Int(10_000).ref_cell()], span()).unwrap().borrow() {
            Value::Bool(b) => assert!(!*b),
            other => panic!("expected bool, got {other:?}"),
        }
        nram_set_limit_mb(&[Value::Int(0).ref_cell()], span()).unwrap();
    }

    #[test]
    fn pressure_is_known_word() {
        match &*nram_pressure(&[], span()).unwrap().borrow() {
            Value::String(s) => {
                assert!(["low", "medium", "high", "critical", "unknown"].contains(&s.as_str()))
            }
            other => panic!("expected string, got {other:?}"),
        }
    }
}
