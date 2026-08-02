//! Native nbatch standard library — adaptive batch sizing for memory-aware
//! training/inference loops: suggest a batch from VRAM/RAM budget, fit steps,
//! clamp/scale, and halve on failure.
//!
//! Import with `import "nbatch"` (or `import "std/nbatch"`).

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E3030_NBATCH_ARITY: u32 = 3030;
const E3031_NBATCH_ERROR: u32 = 3031;
const E3032_NBATCH_TYPE: u32 = 3032;

const DEFAULT_AVAILABLE_MB: f64 = 1024.0;
const DEFAULT_ITEM_BYTES: f64 = 1.0;
const DEFAULT_MAX: i64 = 4096;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3030_NBATCH_ARITY,
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
            E3030_NBATCH_ARITY,
            format!(
                "{name}() expects {min}..{max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3032_NBATCH_TYPE, msg.into())
}

fn batch_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3031_NBATCH_ERROR, msg.into())
}

fn as_number(v: &Value, name: &str, idx: usize, span: Span) -> NiaoResult<f64> {
    match v {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a number as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn number_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    as_number(&args[idx].borrow(), name, idx, span)
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        Value::Float(f) if f.fract() == 0.0 && f.is_finite() => Ok(*f as i64),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an integer as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn bool_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<bool> {
    match &*args[idx].borrow() {
        Value::Bool(b) => Ok(*b),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a bool as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

/// Optional positional number: missing or `nil` → `None`.
fn opt_number_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Option<f64>> {
    if idx >= args.len() {
        return Ok(None);
    }
    match &*args[idx].borrow() {
        Value::Nil => Ok(None),
        other => Ok(Some(as_number(other, name, idx, span)?)),
    }
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

// ---------------------------------------------------------------------------
// Core math
// ---------------------------------------------------------------------------

fn suggest_batch(
    vram_mb: Option<f64>,
    ram_mb: Option<f64>,
    item_bytes: f64,
    max: i64,
    span: Span,
) -> NiaoResult<i64> {
    if item_bytes <= 0.0 || !item_bytes.is_finite() {
        return Err(batch_err(
            span,
            "nbatch_suggest() item_bytes must be a positive finite number",
        ));
    }
    if max < 1 {
        return Err(batch_err(span, "nbatch_suggest() max must be >= 1"));
    }

    let available_mb = match (vram_mb, ram_mb) {
        (Some(v), _) if v > 0.0 && v.is_finite() => v,
        (_, Some(r)) if r > 0.0 && r.is_finite() => r,
        _ => DEFAULT_AVAILABLE_MB,
    };

    let budget_bytes = available_mb * 1024.0 * 1024.0 * 0.5;
    let raw = (budget_bytes / item_bytes).floor();
    if !raw.is_finite() {
        return Err(batch_err(span, "nbatch_suggest() batch size overflow"));
    }
    let batch = raw.clamp(1.0, max as f64) as i64;
    Ok(batch)
}

fn fit_steps(total: i64, batch: i64, span: Span) -> NiaoResult<i64> {
    if batch <= 0 {
        return Err(batch_err(span, "nbatch_fit() batch must be >= 1"));
    }
    if total < 0 {
        return Err(batch_err(span, "nbatch_fit() total must be >= 0"));
    }
    if total == 0 {
        return Ok(0);
    }
    // ceil(total / batch)
    Ok((total + batch - 1) / batch)
}

fn clamp_i64(n: i64, min: i64, max: i64, span: Span) -> NiaoResult<i64> {
    if min > max {
        return Err(batch_err(
            span,
            format!("nbatch_clamp() min ({min}) must be <= max ({max})"),
        ));
    }
    Ok(n.clamp(min, max))
}

fn scale_i64(n: i64, factor: f64, span: Span) -> NiaoResult<i64> {
    if !factor.is_finite() {
        return Err(batch_err(span, "nbatch_scale() factor must be finite"));
    }
    let scaled = (n as f64) * factor;
    if !scaled.is_finite() {
        return Err(batch_err(span, "nbatch_scale() result overflow"));
    }
    Ok(scaled.trunc() as i64)
}

fn halve_on_impl(ok: bool, n: i64) -> i64 {
    if ok {
        n
    } else {
        (n / 2).max(1)
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nbatch_suggest(vram_mb?, ram_mb?, item_bytes?, max?) → int
///
/// `available_mb = vram or ram or 1024`;
/// `batch = floor((available_mb * 1024 * 1024 * 0.5) / item_bytes)` clamped to `1..max`
/// (`max` defaults to 4096; `item_bytes` defaults to 1).
fn nbatch_suggest(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 4, "nbatch_suggest", span)?;
    let vram_mb = opt_number_arg(args, 0, "nbatch_suggest", span)?;
    let ram_mb = opt_number_arg(args, 1, "nbatch_suggest", span)?;
    let item_bytes = opt_number_arg(args, 2, "nbatch_suggest", span)?.unwrap_or(DEFAULT_ITEM_BYTES);
    let max = match opt_number_arg(args, 3, "nbatch_suggest", span)? {
        Some(m) => {
            if m.fract() != 0.0 || m < 1.0 || !m.is_finite() {
                return Err(batch_err(
                    span,
                    "nbatch_suggest() max must be an integer >= 1",
                ));
            }
            m as i64
        }
        None => DEFAULT_MAX,
    };
    int_val(suggest_batch(vram_mb, ram_mb, item_bytes, max, span)?)
}

/// nbatch_fit(total, batch) → ceil(total / batch) steps
fn nbatch_fit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbatch_fit", span)?;
    let total = int_arg(args, 0, "nbatch_fit", span)?;
    let batch = int_arg(args, 1, "nbatch_fit", span)?;
    int_val(fit_steps(total, batch, span)?)
}

/// nbatch_clamp(n, min, max)
fn nbatch_clamp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nbatch_clamp", span)?;
    let n = int_arg(args, 0, "nbatch_clamp", span)?;
    let min = int_arg(args, 1, "nbatch_clamp", span)?;
    let max = int_arg(args, 2, "nbatch_clamp", span)?;
    int_val(clamp_i64(n, min, max, span)?)
}

/// nbatch_scale(n, factor) → trunc(n * factor)
fn nbatch_scale(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbatch_scale", span)?;
    let n = int_arg(args, 0, "nbatch_scale", span)?;
    let factor = number_arg(args, 1, "nbatch_scale", span)?;
    int_val(scale_i64(n, factor, span)?)
}

/// nbatch_halve_on(ok_bool, n) — if !ok return max(1, n/2), else n
fn nbatch_halve_on(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbatch_halve_on", span)?;
    let ok = bool_arg(args, 0, "nbatch_halve_on", span)?;
    let n = int_arg(args, 1, "nbatch_halve_on", span)?;
    int_val(halve_on_impl(ok, n))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nbatch_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nbatch_fns![
    ("nbatch_suggest", "suggest", nbatch_suggest),
    ("nbatch_fit", "fit", nbatch_fit),
    ("nbatch_clamp", "clamp", nbatch_clamp),
    ("nbatch_scale", "scale", nbatch_scale),
    ("nbatch_halve_on", "halve_on", nbatch_halve_on),
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

pub const MODULE_NAME: &str = "nbatch";
pub const MODULE_PATHS: &[&str] = &["nbatch", "std/nbatch"];

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

    fn call(f: fn(&[ValueRef], Span) -> NiaoResult<ValueRef>, args: Vec<Value>) -> Value {
        let refs: Vec<ValueRef> = args.into_iter().map(|v| v.ref_cell()).collect();
        f(&refs, span()).unwrap().borrow().clone()
    }

    fn call_err(
        f: fn(&[ValueRef], Span) -> NiaoResult<ValueRef>,
        args: Vec<Value>,
    ) -> RuntimeError {
        let refs: Vec<ValueRef> = args.into_iter().map(|v| v.ref_cell()).collect();
        f(&refs, span()).unwrap_err()
    }

    #[test]
    fn suggest_defaults_and_vram() {
        // default available 1024 MB → budget 512 MiB / 1 byte → huge → clamp 4096
        assert!(matches!(call(nbatch_suggest, vec![]), Value::Int(4096)));

        // 8 GiB VRAM, 4 MiB items: budget = 8*1024^2*0.5 = 4 GiB → 1024 items
        let batch = call(
            nbatch_suggest,
            vec![Value::Int(8192), Value::Nil, Value::Int(4 * 1024 * 1024)],
        );
        assert!(matches!(batch, Value::Int(1024)));

        // prefer vram over ram
        let batch = call(
            nbatch_suggest,
            vec![
                Value::Int(2048),
                Value::Int(65536),
                Value::Int(1024 * 1024),
                Value::Int(10000),
            ],
        );
        // budget = 2048*1024^2*0.5 / 1MiB = 1024
        assert!(matches!(batch, Value::Int(1024)));

        // ram when vram unset
        let batch = call(
            nbatch_suggest,
            vec![Value::Nil, Value::Int(4096), Value::Int(1024 * 1024)],
        );
        // budget = 4096*1024^2*0.5 / 1MiB = 2048
        assert!(matches!(batch, Value::Int(2048)));
    }

    #[test]
    fn suggest_clamps_to_max_and_one() {
        let batch = call(
            nbatch_suggest,
            vec![Value::Int(8192), Value::Nil, Value::Int(1), Value::Int(64)],
        );
        assert!(matches!(batch, Value::Int(64)));

        // tiny budget relative to huge items → clamp to 1
        let batch = call(
            nbatch_suggest,
            vec![
                Value::Float(0.001),
                Value::Nil,
                Value::Int(1024 * 1024 * 1024),
            ],
        );
        assert!(matches!(batch, Value::Int(1)));
    }

    #[test]
    fn fit_ceil_steps() {
        assert!(matches!(
            call(nbatch_fit, vec![Value::Int(100), Value::Int(32)]),
            Value::Int(4)
        ));
        assert!(matches!(
            call(nbatch_fit, vec![Value::Int(96), Value::Int(32)]),
            Value::Int(3)
        ));
        assert!(matches!(
            call(nbatch_fit, vec![Value::Int(0), Value::Int(32)]),
            Value::Int(0)
        ));
    }

    #[test]
    fn clamp_scale_halve() {
        assert!(matches!(
            call(
                nbatch_clamp,
                vec![Value::Int(50), Value::Int(1), Value::Int(32)]
            ),
            Value::Int(32)
        ));
        assert!(matches!(
            call(
                nbatch_clamp,
                vec![Value::Int(0), Value::Int(1), Value::Int(32)]
            ),
            Value::Int(1)
        ));
        assert!(matches!(
            call(nbatch_scale, vec![Value::Int(64), Value::Float(0.5)]),
            Value::Int(32)
        ));
        assert!(matches!(
            call(nbatch_scale, vec![Value::Int(10), Value::Float(1.5)]),
            Value::Int(15)
        ));
        assert!(matches!(
            call(nbatch_halve_on, vec![Value::Bool(true), Value::Int(128)]),
            Value::Int(128)
        ));
        assert!(matches!(
            call(nbatch_halve_on, vec![Value::Bool(false), Value::Int(128)]),
            Value::Int(64)
        ));
        assert!(matches!(
            call(nbatch_halve_on, vec![Value::Bool(false), Value::Int(1)]),
            Value::Int(1)
        ));
    }

    #[test]
    fn errors_on_bad_arity_and_types() {
        match call_err(nbatch_fit, vec![Value::Int(1)]) {
            RuntimeError::Generic { code, .. } => assert_eq!(code, E3030_NBATCH_ARITY),
            other => panic!("expected arity error, got {other:?}"),
        }
        match call_err(nbatch_halve_on, vec![Value::Int(1), Value::Int(8)]) {
            RuntimeError::Generic { code, .. } => assert_eq!(code, E3032_NBATCH_TYPE),
            other => panic!("expected type error, got {other:?}"),
        }
        match call_err(nbatch_fit, vec![Value::Int(10), Value::Int(0)]) {
            RuntimeError::Generic { code, .. } => assert_eq!(code, E3031_NBATCH_ERROR),
            other => panic!("expected batch error, got {other:?}"),
        }
        match call_err(nbatch_suggest, vec![Value::Nil, Value::Nil, Value::Int(0)]) {
            RuntimeError::Generic { code, .. } => assert_eq!(code, E3031_NBATCH_ERROR),
            other => panic!("expected batch error, got {other:?}"),
        }
    }

    #[test]
    fn namespace_exposes_short_names() {
        match namespace() {
            Value::Object(map) => {
                for key in ["suggest", "fit", "clamp", "scale", "halve_on"] {
                    assert!(map.contains_key(key), "missing {key}");
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
