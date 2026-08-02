//! Native nfallback standard library — graceful degradation chains:
//! first/coalesce/or over nil and error values, plus a named token-failure
//! circuit breaker with auto-reset.
//!
//! Import with `import "nfallback"` (or `import "std/nfallback"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

// Wired into codes.rs by central integration.
const E3040_NFALLBACK_ARITY: u32 = 3040;
const E3041_NFALLBACK_ERROR: u32 = 3041;
const E3042_NFALLBACK_TYPE: u32 = 3042;

const DEFAULT_THRESHOLD: i64 = 5;
const DEFAULT_RESET_MS: i64 = 30_000;

// ---------------------------------------------------------------------------
// Circuit breaker state
// ---------------------------------------------------------------------------

struct Circuit {
    fails: i64,
    threshold: i64,
    opened_at_ms: Option<i64>,
    reset_ms: i64,
}

impl Circuit {
    fn new(threshold: i64, reset_ms: i64) -> Self {
        Circuit {
            fails: 0,
            threshold,
            opened_at_ms: None,
            reset_ms,
        }
    }

    /// Auto-close when `reset_ms` has elapsed since open (`reset_ms == 0` = never).
    fn maybe_auto_close(&mut self, now_ms: i64) {
        if let Some(opened) = self.opened_at_ms {
            if self.reset_ms > 0 && now_ms.saturating_sub(opened) >= self.reset_ms {
                self.opened_at_ms = None;
                self.fails = 0;
            }
        }
    }

    fn is_open(&mut self, now_ms: i64) -> bool {
        self.maybe_auto_close(now_ms);
        self.opened_at_ms.is_some()
    }

    fn force_close(&mut self) {
        self.opened_at_ms = None;
        self.fails = 0;
    }

    /// Record an outcome. Returns `true` if the circuit is closed (allowing).
    fn record(&mut self, success: bool, now_ms: i64) -> bool {
        self.maybe_auto_close(now_ms);
        if self.opened_at_ms.is_some() {
            return false;
        }
        if success {
            self.fails = 0;
        } else {
            self.fails = self.fails.saturating_add(1);
            if self.fails >= self.threshold {
                self.opened_at_ms = Some(now_ms);
                return false;
            }
        }
        true
    }
}

thread_local! {
    static CIRCUITS: RefCell<HashMap<String, Circuit>> = RefCell::new(HashMap::new());
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3040_NFALLBACK_ARITY,
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
            E3040_NFALLBACK_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3042_NFALLBACK_TYPE, msg.into())
}

fn nfallback_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3041_NFALLBACK_ERROR, "nfallback_error", msg.into(), span)
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

fn array_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<ValueRef>> {
    match &*args[idx].borrow() {
        Value::Array(a) => Ok(a.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn is_usable(v: &ValueRef) -> bool {
    !matches!(&*v.borrow(), Value::Nil | Value::Error(_))
}

fn first_usable(values: &[ValueRef]) -> ValueRef {
    for v in values {
        if is_usable(v) {
            return Rc::clone(v);
        }
    }
    Value::Nil.ref_cell()
}

fn opt_i64_field(
    map: &HashMap<String, ValueRef>,
    keys: &[&str],
    span: Span,
) -> NiaoResult<Option<i64>> {
    for key in keys {
        if let Some(v) = map.get(*key) {
            return match &*v.borrow() {
                Value::Nil => Ok(None),
                Value::Int(n) => Ok(Some(*n)),
                other => Err(type_err(
                    span,
                    format!(
                        "nfallback_circuit() opts '{key}' expects an int, got {}",
                        other.type_name()
                    ),
                )),
            };
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// first(array) — first non-nil, non-Error value (else nil).
fn nfallback_first(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfallback_first", span)?;
    let arr = array_arg(args, 0, "nfallback_first", span)?;
    Ok(first_usable(&arr))
}

/// coalesce(v1, …, v16) — first usable among varargs.
fn nfallback_coalesce(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 16, "nfallback_coalesce", span)?;
    Ok(first_usable(args))
}

/// or(a, b) — prefer `a` unless nil/Error.
fn nfallback_or(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfallback_or", span)?;
    if is_usable(&args[0]) {
        Ok(Rc::clone(&args[0]))
    } else {
        Ok(Rc::clone(&args[1]))
    }
}

/// try_chain(array) — alias of first.
fn nfallback_try_chain(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfallback_try_chain", span)?;
    let arr = array_arg(args, 0, "nfallback_try_chain", span)?;
    Ok(first_usable(&arr))
}

/// circuit(name, success_bool, opts?) — record outcome; returns true if closed.
///
/// opts: `{threshold?: int, fail_threshold?: int, reset_ms?: int}`
/// Defaults: threshold=5, reset_ms=30000.
fn nfallback_circuit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nfallback_circuit", span)?;
    let name = string_arg(args, 0, "nfallback_circuit", span)?;
    let success = bool_arg(args, 1, "nfallback_circuit", span)?;

    let mut threshold = DEFAULT_THRESHOLD;
    let mut reset_ms = DEFAULT_RESET_MS;
    if args.len() > 2 {
        match &*args[2].borrow() {
            Value::Object(map) => {
                if let Some(t) = opt_i64_field(map, &["threshold", "fail_threshold"], span)? {
                    if t < 1 {
                        return Ok(nfallback_err(
                            span,
                            "nfallback_circuit() threshold must be >= 1",
                        ));
                    }
                    threshold = t;
                }
                if let Some(r) = opt_i64_field(map, &["reset_ms"], span)? {
                    if r < 0 {
                        return Ok(nfallback_err(
                            span,
                            "nfallback_circuit() reset_ms must be >= 0",
                        ));
                    }
                    reset_ms = r;
                }
            }
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "nfallback_circuit() expects an object as opts, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    }

    let now = now_ms();
    let closed = CIRCUITS.with(|map| {
        let mut map = map.borrow_mut();
        let c = map
            .entry(name)
            .or_insert_with(|| Circuit::new(threshold, reset_ms));
        // Refresh knobs on each call so callers can retune.
        c.threshold = threshold;
        c.reset_ms = reset_ms;
        c.record(success, now)
    });
    Ok(Value::Bool(closed).ref_cell())
}

fn nfallback_is_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfallback_is_open", span)?;
    let name = string_arg(args, 0, "nfallback_is_open", span)?;
    let now = now_ms();
    let open = CIRCUITS.with(|map| {
        let mut map = map.borrow_mut();
        match map.get_mut(&name) {
            Some(c) => c.is_open(now),
            None => false,
        }
    });
    Ok(Value::Bool(open).ref_cell())
}

fn nfallback_reset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfallback_reset", span)?;
    let name = string_arg(args, 0, "nfallback_reset", span)?;
    CIRCUITS.with(|map| {
        let mut map = map.borrow_mut();
        if let Some(c) = map.get_mut(&name) {
            c.force_close();
        }
    });
    Ok(Value::Nil.ref_cell())
}

/// allow(name) — force-close the circuit (same effect as reset for openness).
fn nfallback_allow(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfallback_allow", span)?;
    let name = string_arg(args, 0, "nfallback_allow", span)?;
    CIRCUITS.with(|map| {
        let mut map = map.borrow_mut();
        if let Some(c) = map.get_mut(&name) {
            c.force_close();
        }
    });
    Ok(Value::Nil.ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nfallback_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nfallback_fns![
    ("nfallback_first", "first", nfallback_first),
    ("nfallback_coalesce", "coalesce", nfallback_coalesce),
    ("nfallback_or", "or", nfallback_or),
    ("nfallback_try_chain", "try_chain", nfallback_try_chain),
    ("nfallback_circuit", "circuit", nfallback_circuit),
    ("nfallback_is_open", "is_open", nfallback_is_open),
    ("nfallback_reset", "reset", nfallback_reset),
    ("nfallback_allow", "allow", nfallback_allow),
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

pub const MODULE_NAME: &str = "nfallback";
pub const MODULE_PATHS: &[&str] = &["nfallback", "std/nfallback"];

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

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn b(v: bool) -> ValueRef {
        Value::Bool(v).ref_cell()
    }

    fn nil() -> ValueRef {
        Value::Nil.ref_cell()
    }

    fn err() -> ValueRef {
        error_value(E3041_NFALLBACK_ERROR, "nfallback_error", "boom", span())
    }

    fn arr(vals: Vec<ValueRef>) -> ValueRef {
        Value::Array(vals).ref_cell()
    }

    fn clear_circuits() {
        CIRCUITS.with(|m| m.borrow_mut().clear());
    }

    #[test]
    fn first_skips_nil_and_error() {
        let v = nfallback_first(&[arr(vec![nil(), err(), i(42), i(7)])], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(42)));
    }

    #[test]
    fn first_all_bad_is_nil() {
        let v = nfallback_first(&[arr(vec![nil(), err()])], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Nil));
    }

    #[test]
    fn coalesce_varargs() {
        let v = nfallback_coalesce(&[nil(), err(), s("ok")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::String(x) if x == "ok"));
    }

    #[test]
    fn coalesce_arity_bounds() {
        assert!(nfallback_coalesce(&[], span()).is_err());
        let many: Vec<ValueRef> = (0..17).map(i).collect();
        assert!(nfallback_coalesce(&many, span()).is_err());
    }

    #[test]
    fn or_prefers_usable() {
        let a = nfallback_or(&[i(1), i(2)], span()).unwrap();
        assert!(matches!(&*a.borrow(), Value::Int(1)));
        let b = nfallback_or(&[nil(), i(2)], span()).unwrap();
        assert!(matches!(&*b.borrow(), Value::Int(2)));
        let c = nfallback_or(&[err(), s("b")], span()).unwrap();
        assert!(matches!(&*c.borrow(), Value::String(x) if x == "b"));
    }

    #[test]
    fn try_chain_matches_first() {
        let v = nfallback_try_chain(&[arr(vec![err(), nil(), i(9)])], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(9)));
    }

    #[test]
    fn circuit_opens_at_threshold() {
        clear_circuits();
        let name = s("svc");
        for _ in 0..4 {
            let r = nfallback_circuit(&[name.clone(), b(false)], span()).unwrap();
            assert!(matches!(&*r.borrow(), Value::Bool(true)));
            assert!(!matches!(
                &*nfallback_is_open(&[name.clone()], span()).unwrap().borrow(),
                Value::Bool(true)
            ));
        }
        let r = nfallback_circuit(&[name.clone(), b(false)], span()).unwrap();
        assert!(matches!(&*r.borrow(), Value::Bool(false)));
        assert!(matches!(
            &*nfallback_is_open(&[name.clone()], span()).unwrap().borrow(),
            Value::Bool(true)
        ));

        // Still open — further calls return false.
        let r2 = nfallback_circuit(&[name.clone(), b(true)], span()).unwrap();
        assert!(matches!(&*r2.borrow(), Value::Bool(false)));

        nfallback_allow(&[name.clone()], span()).unwrap();
        assert!(matches!(
            &*nfallback_is_open(&[name], span()).unwrap().borrow(),
            Value::Bool(false)
        ));
    }

    #[test]
    fn circuit_custom_threshold_and_reset() {
        clear_circuits();
        let mut opts = HashMap::new();
        opts.insert("threshold".into(), i(2));
        opts.insert("reset_ms".into(), i(0)); // never auto-close
        let opts = Value::Object(opts).ref_cell();
        let name = s("db");
        nfallback_circuit(&[name.clone(), b(false), opts.clone()], span()).unwrap();
        let r = nfallback_circuit(&[name.clone(), b(false), opts], span()).unwrap();
        assert!(matches!(&*r.borrow(), Value::Bool(false)));
        assert!(matches!(
            &*nfallback_is_open(&[name.clone()], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
        nfallback_reset(&[name], span()).unwrap();
    }

    #[test]
    fn success_clears_fail_count() {
        clear_circuits();
        let name = s("api");
        nfallback_circuit(&[name.clone(), b(false)], span()).unwrap();
        nfallback_circuit(&[name.clone(), b(false)], span()).unwrap();
        nfallback_circuit(&[name.clone(), b(true)], span()).unwrap();
        // After success, need 5 more fails to open.
        for _ in 0..4 {
            nfallback_circuit(&[name.clone(), b(false)], span()).unwrap();
        }
        assert!(matches!(
            &*nfallback_is_open(&[name.clone()], span()).unwrap().borrow(),
            Value::Bool(false)
        ));
        nfallback_circuit(&[name.clone(), b(false)], span()).unwrap();
        assert!(matches!(
            &*nfallback_is_open(&[name], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
    }

    #[test]
    fn type_errors() {
        assert!(nfallback_first(&[i(1)], span()).is_err());
        assert!(nfallback_circuit(&[i(1), b(true)], span()).is_err());
        assert!(nfallback_circuit(&[s("x"), i(1)], span()).is_err());
    }
}
