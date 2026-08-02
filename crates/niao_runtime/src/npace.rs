//! Native npace standard library — adaptive loop pacing.
//! Thread-local pace level (0..=3) maps to sleep delays 0/2/8/25 ms.
//! Helpers map temperature and load percent into a level.
//!
//! Import with `import "npace"` (or `import "std/npace"`).
//!
//! Note: `with_level` is omitted — Niao native builtins cannot take callable
//! function arguments.

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::thread;
use std::time::Duration;

// Wired into niao_errors::codes by central integration.
const E3020_NPACE_ARITY: u32 = 3020;
const E3021_NPACE_ERROR: u32 = 3021;
const E3022_NPACE_TYPE: u32 = 3022;

/// Delay in milliseconds for each pace level 0..=3.
const DELAYS_MS: [i64; 4] = [0, 2, 8, 25];

thread_local! {
    static LEVEL: Cell<i64> = Cell::new(0);
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3022_NPACE_TYPE, msg.into())
}

fn pace_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3021_NPACE_ERROR, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3020_NPACE_ARITY,
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

fn number_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    match &*args[idx].borrow() {
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

fn delay_for(level: i64) -> i64 {
    DELAYS_MS[level as usize]
}

fn get_level() -> i64 {
    LEVEL.with(|c| c.get())
}

fn set_level_raw(level: i64) {
    LEVEL.with(|c| c.set(level));
}

/// Map temperature `c` relative to `max` into pace level 0..=3.
/// Cooler → lower level; at/above `max` → 3.
fn level_from_temp(c: f64, max: f64) -> Result<i64, String> {
    if !c.is_finite() || !max.is_finite() {
        return Err("from_temp() expects finite numbers".into());
    }
    if max <= 0.0 {
        return Err("from_temp() expects max > 0".into());
    }
    let ratio = (c / max).clamp(0.0, 1.0);
    let level = (ratio * 4.0).floor().min(3.0) as i64;
    Ok(level)
}

/// Map load percent into pace level 0..=3 (0–24→0, 25–49→1, 50–74→2, 75+→3).
fn level_from_load(pct: f64) -> Result<i64, String> {
    if !pct.is_finite() {
        return Err("from_load() expects a finite number".into());
    }
    let p = pct.clamp(0.0, 100.0);
    Ok((p / 25.0).floor().min(3.0) as i64)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn npace_set_level(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npace_set_level", span)?;
    let level = int_arg(args, 0, "npace_set_level", span)?;
    if !(0..=3).contains(&level) {
        return Err(pace_err(
            span,
            format!("npace_set_level() expects level 0..=3, got {level}"),
        ));
    }
    set_level_raw(level);
    Ok(Value::Int(level).ref_cell())
}

fn npace_level(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "npace_level", span)?;
    Ok(Value::Int(get_level()).ref_cell())
}

fn npace_sleep_ms(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "npace_sleep_ms", span)?;
    Ok(Value::Int(delay_for(get_level())).ref_cell())
}

fn npace_tick(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "npace_tick", span)?;
    let ms = delay_for(get_level());
    if ms > 0 {
        thread::sleep(Duration::from_millis(ms as u64));
    }
    Ok(Value::Int(ms).ref_cell())
}

fn npace_from_temp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "npace_from_temp", span)?;
    let c = number_arg(args, 0, "npace_from_temp", span)?;
    let max = number_arg(args, 1, "npace_from_temp", span)?;
    match level_from_temp(c, max) {
        Ok(level) => Ok(Value::Int(level).ref_cell()),
        Err(msg) => Err(pace_err(span, msg)),
    }
}

fn npace_from_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npace_from_load", span)?;
    let pct = number_arg(args, 0, "npace_from_load", span)?;
    match level_from_load(pct) {
        Ok(level) => Ok(Value::Int(level).ref_cell()),
        Err(msg) => Err(pace_err(span, msg)),
    }
}

fn npace_delays(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "npace_delays", span)?;
    let mut map = HashMap::new();
    for (level, ms) in DELAYS_MS.iter().enumerate() {
        map.insert(level.to_string(), Value::Int(*ms).ref_cell());
    }
    Ok(Value::Object(map).ref_cell())
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

macro_rules! npace_fns {
    ($(($flat:expr, $short:expr, $f:expr)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

npace_fns![
    ("npace_set_level", "set_level", npace_set_level),
    ("npace_level", "level", npace_level),
    ("npace_sleep_ms", "sleep_ms", npace_sleep_ms),
    ("npace_tick", "tick", npace_tick),
    ("npace_from_temp", "from_temp", npace_from_temp),
    ("npace_from_load", "from_load", npace_from_load),
    ("npace_delays", "delays", npace_delays),
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

pub const MODULE_NAME: &str = "npace";
pub const MODULE_PATHS: &[&str] = &["npace", "std/npace"];

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

    fn reset() {
        set_level_raw(0);
    }

    #[test]
    fn delays_table() {
        assert_eq!(delay_for(0), 0);
        assert_eq!(delay_for(1), 2);
        assert_eq!(delay_for(2), 8);
        assert_eq!(delay_for(3), 25);
    }

    #[test]
    fn set_and_get_level() {
        reset();
        let v = npace_set_level(&[Value::Int(2).ref_cell()], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(2)));
        let lvl = npace_level(&[], span()).unwrap();
        assert!(matches!(&*lvl.borrow(), Value::Int(2)));
        let ms = npace_sleep_ms(&[], span()).unwrap();
        assert!(matches!(&*ms.borrow(), Value::Int(8)));
        reset();
    }

    #[test]
    fn set_level_rejects_out_of_range() {
        reset();
        let err = npace_set_level(&[Value::Int(4).ref_cell()], span()).unwrap_err();
        assert_eq!(err.code(), E3021_NPACE_ERROR);
        let err = npace_set_level(&[Value::Int(-1).ref_cell()], span()).unwrap_err();
        assert_eq!(err.code(), E3021_NPACE_ERROR);
    }

    #[test]
    fn set_level_type_and_arity() {
        let err = npace_set_level(&[], span()).unwrap_err();
        assert_eq!(err.code(), E3020_NPACE_ARITY);
        let err = npace_set_level(&[Value::String("hot".into()).ref_cell()], span()).unwrap_err();
        assert_eq!(err.code(), E3022_NPACE_TYPE);
    }

    #[test]
    fn tick_at_level_zero_is_instant() {
        reset();
        let v = npace_tick(&[], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(0)));
    }

    #[test]
    fn from_temp_mapping() {
        assert_eq!(level_from_temp(0.0, 100.0).unwrap(), 0);
        assert_eq!(level_from_temp(24.0, 100.0).unwrap(), 0);
        assert_eq!(level_from_temp(25.0, 100.0).unwrap(), 1);
        assert_eq!(level_from_temp(49.0, 100.0).unwrap(), 1);
        assert_eq!(level_from_temp(50.0, 100.0).unwrap(), 2);
        assert_eq!(level_from_temp(74.0, 100.0).unwrap(), 2);
        assert_eq!(level_from_temp(75.0, 100.0).unwrap(), 3);
        assert_eq!(level_from_temp(100.0, 100.0).unwrap(), 3);
        assert_eq!(level_from_temp(200.0, 100.0).unwrap(), 3);
        assert!(level_from_temp(50.0, 0.0).is_err());
    }

    #[test]
    fn from_load_mapping() {
        assert_eq!(level_from_load(0.0).unwrap(), 0);
        assert_eq!(level_from_load(24.0).unwrap(), 0);
        assert_eq!(level_from_load(25.0).unwrap(), 1);
        assert_eq!(level_from_load(50.0).unwrap(), 2);
        assert_eq!(level_from_load(75.0).unwrap(), 3);
        assert_eq!(level_from_load(100.0).unwrap(), 3);
        assert_eq!(level_from_load(150.0).unwrap(), 3);
        assert_eq!(level_from_load(-10.0).unwrap(), 0);
    }

    #[test]
    fn from_temp_and_load_builtins() {
        let v = npace_from_temp(
            &[Value::Float(80.0).ref_cell(), Value::Int(100).ref_cell()],
            span(),
        )
        .unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(3)));

        let v = npace_from_load(&[Value::Int(40).ref_cell()], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(1)));
    }

    #[test]
    fn delays_object() {
        let v = npace_delays(&[], span()).unwrap();
        match &*v.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map.get("0").unwrap().borrow(), Value::Int(0)));
                assert!(matches!(&*map.get("1").unwrap().borrow(), Value::Int(2)));
                assert!(matches!(&*map.get("2").unwrap().borrow(), Value::Int(8)));
                assert!(matches!(&*map.get("3").unwrap().borrow(), Value::Int(25)));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn namespace_has_short_names() {
        match namespace() {
            Value::Object(map) => {
                for key in [
                    "set_level",
                    "level",
                    "sleep_ms",
                    "tick",
                    "from_temp",
                    "from_load",
                    "delays",
                ] {
                    assert!(map.contains_key(key), "missing {key}");
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
