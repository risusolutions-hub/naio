//! Native nbudget standard library — unified cooperative resource and cost
//! budgets. Track cpu_pct / ram_mb / gpu_pct / usd / tokens limits, charge
//! usage, and soft-check remaining headroom. No OS enforcement.
//!
//! Import with `import "nbudget"` (or `import "std/nbudget"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Budget model
// ---------------------------------------------------------------------------

const KINDS: &[&str] = &["cpu_pct", "ram_mb", "gpu_pct", "usd", "tokens"];

#[derive(Clone, Copy, Default)]
struct Amounts {
    cpu_pct: Option<f64>,
    ram_mb: Option<f64>,
    gpu_pct: Option<f64>,
    usd: Option<f64>,
    tokens: Option<f64>,
}

impl Amounts {
    fn get(&self, kind: &str) -> Option<f64> {
        match kind {
            "cpu_pct" => self.cpu_pct,
            "ram_mb" => self.ram_mb,
            "gpu_pct" => self.gpu_pct,
            "usd" => self.usd,
            "tokens" => self.tokens,
            _ => None,
        }
    }

    fn set_field(&mut self, kind: &str, value: f64) {
        match kind {
            "cpu_pct" => self.cpu_pct = Some(value),
            "ram_mb" => self.ram_mb = Some(value),
            "gpu_pct" => self.gpu_pct = Some(value),
            "usd" => self.usd = Some(value),
            "tokens" => self.tokens = Some(value),
            _ => {}
        }
    }

    fn add(&mut self, kind: &str, amount: f64) {
        let cur = self.get(kind).unwrap_or(0.0);
        self.set_field(kind, cur + amount);
    }

    fn clear(&mut self) {
        *self = Amounts::default();
    }

    fn to_object(&self, omit_none: bool) -> HashMap<String, ValueRef> {
        let mut map = HashMap::new();
        for &kind in KINDS {
            match self.get(kind) {
                Some(n) => {
                    map.insert(kind.to_string(), num_val(n));
                }
                None if !omit_none => {
                    map.insert(kind.to_string(), Value::Nil.ref_cell());
                }
                None => {}
            }
        }
        map
    }
}

#[derive(Default)]
struct BudgetState {
    limits: Amounts,
    used: Amounts,
}

thread_local! {
    static STATE: RefCell<BudgetState> = RefCell::new(BudgetState::default());
}

fn with_state<T>(f: impl FnOnce(&mut BudgetState) -> T) -> T {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn num_val(n: f64) -> ValueRef {
    if n.is_finite() && n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
        Value::Int(n as i64).ref_cell()
    } else {
        Value::Float(n).ref_cell()
    }
}

fn value_as_number(v: &Value, span: Span, ctx: &str) -> NiaoResult<f64> {
    match v {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => {
            if !f.is_finite() {
                return Err(RuntimeError::at(
                    span,
                    codes::E2942_NBUDGET_TYPE,
                    format!("{ctx}: expected a finite number, got non-finite float"),
                ));
            }
            Ok(*f)
        }
        other => Err(RuntimeError::at(
            span,
            codes::E2942_NBUDGET_TYPE,
            format!("{ctx}: expected a number, got {}", other.type_name()),
        )),
    }
}

fn parse_kind(kind: &str, span: Span) -> NiaoResult<&'static str> {
    for &k in KINDS {
        if k == kind {
            return Ok(k);
        }
    }
    Err(RuntimeError::at(
        span,
        codes::E2941_NBUDGET_ERROR,
        format!(
            "unknown budget kind '{kind}'; expected one of cpu_pct|ram_mb|gpu_pct|usd|tokens"
        ),
    ))
}

fn parse_amounts_obj(
    map: &HashMap<String, ValueRef>,
    span: Span,
    ctx: &str,
) -> NiaoResult<Amounts> {
    let mut out = Amounts::default();
    for (key, val) in map {
        if !KINDS.contains(&key.as_str()) {
            return Err(RuntimeError::at(
                span,
                codes::E2941_NBUDGET_ERROR,
                format!("{ctx}: unknown key '{key}'; expected cpu_pct|ram_mb|gpu_pct|usd|tokens"),
            ));
        }
        match &*val.borrow() {
            Value::Nil => {}
            other => {
                let n = value_as_number(other, span, &format!("{ctx}.{key}"))?;
                if n < 0.0 {
                    return Err(RuntimeError::at(
                        span,
                        codes::E2941_NBUDGET_ERROR,
                        format!("{ctx}.{key}: amount must be >= 0, got {n}"),
                    ));
                }
                out.set_field(key, n);
            }
        }
    }
    Ok(out)
}

fn collect_violations(limits: &Amounts, used: &Amounts, extra: &Amounts) -> Vec<String> {
    let mut violations = Vec::new();
    for &kind in KINDS {
        let Some(limit) = limits.get(kind) else {
            continue;
        };
        let total = used.get(kind).unwrap_or(0.0) + extra.get(kind).unwrap_or(0.0);
        if total > limit {
            violations.push(format!(
                "{kind}: used {total} exceeds limit {limit}"
            ));
        }
    }
    violations
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E2942_NBUDGET_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E2940_NBUDGET_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E2940_NBUDGET_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
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

fn float_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    value_as_number(
        &*args[idx].borrow(),
        span,
        &format!("{name}() argument {}", idx + 1),
    )
}

fn object_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[idx].borrow() {
        Value::Object(m) => Ok(m.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn exceed_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(
        codes::E2943_NBUDGET_EXCEED,
        "nbudget_error",
        msg.into(),
        span,
    )
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nbudget_set(obj) — replace global limits from optional number fields.
fn nbudget_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbudget_set", span)?;
    let map = object_arg(args, 0, "nbudget_set", span)?;
    let limits = parse_amounts_obj(&map, span, "nbudget_set")?;
    with_state(|s| s.limits = limits);
    Ok(Value::Nil.ref_cell())
}

/// nbudget_get() — current limits (unset keys omitted).
fn nbudget_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nbudget_get", span)?;
    let map = with_state(|s| s.limits.to_object(true));
    Ok(Value::Object(map).ref_cell())
}

/// nbudget_clear() — clear all limits (usage counters kept).
fn nbudget_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nbudget_clear", span)?;
    with_state(|s| s.limits.clear());
    Ok(Value::Nil.ref_cell())
}

/// nbudget_check(extra?) — soft check used (+ optional proposed) vs limits.
fn nbudget_check(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nbudget_check", span)?;
    let extra = if args.is_empty() {
        Amounts::default()
    } else {
        let map = object_arg(args, 0, "nbudget_check", span)?;
        parse_amounts_obj(&map, span, "nbudget_check")?
    };
    let violations = with_state(|s| collect_violations(&s.limits, &s.used, &extra));
    let ok = violations.is_empty();
    let mut map = HashMap::new();
    map.insert("ok".to_string(), Value::Bool(ok).ref_cell());
    let arr: Vec<ValueRef> = violations
        .into_iter()
        .map(|v| Value::String(v).ref_cell())
        .collect();
    map.insert("violations".to_string(), Value::Array(arr).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

/// nbudget_ok() — true when current used is within all set limits.
fn nbudget_ok(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nbudget_ok", span)?;
    let ok = with_state(|s| collect_violations(&s.limits, &s.used, &Amounts::default()).is_empty());
    Ok(Value::Bool(ok).ref_cell())
}

/// nbudget_remain() — limit − used for each set limit (may be negative).
fn nbudget_remain(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nbudget_remain", span)?;
    let map = with_state(|s| {
        let mut out = HashMap::new();
        for &kind in KINDS {
            if let Some(limit) = s.limits.get(kind) {
                let used = s.used.get(kind).unwrap_or(0.0);
                out.insert(kind.to_string(), num_val(limit - used));
            }
        }
        out
    });
    Ok(Value::Object(map).ref_cell())
}

/// nbudget_charge(kind, amount) — accumulate usage; catchable exceed if now over limit.
fn nbudget_charge(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbudget_charge", span)?;
    let kind = string_arg(args, 0, "nbudget_charge", span)?;
    let kind = parse_kind(&kind, span)?;
    let amount = float_arg(args, 1, "nbudget_charge", span)?;
    if amount < 0.0 {
        return Err(RuntimeError::at(
            span,
            codes::E2941_NBUDGET_ERROR,
            format!("nbudget_charge() amount must be >= 0, got {amount}"),
        ));
    }
    with_state(|s| {
        s.used.add(kind, amount);
        if let Some(limit) = s.limits.get(kind) {
            let used = s.used.get(kind).unwrap_or(0.0);
            if used > limit {
                return Ok(exceed_err(
                    span,
                    format!(
                        "nbudget_charge({kind}, {amount}): used {used} exceeds limit {limit}"
                    ),
                ));
            }
        }
        Ok(Value::Nil.ref_cell())
    })
}

/// nbudget_used() — charged amounts (zero keys omitted).
fn nbudget_used(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nbudget_used", span)?;
    let map = with_state(|s| {
        let mut out = HashMap::new();
        for &kind in KINDS {
            if let Some(n) = s.used.get(kind) {
                if n != 0.0 {
                    out.insert(kind.to_string(), num_val(n));
                }
            }
        }
        out
    });
    Ok(Value::Object(map).ref_cell())
}

/// nbudget_reset_used() — zero all usage counters.
fn nbudget_reset_used(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nbudget_reset_used", span)?;
    with_state(|s| s.used.clear());
    Ok(Value::Nil.ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nbudget_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nbudget_fns![
    ("nbudget_set", "set", nbudget_set),
    ("nbudget_get", "get", nbudget_get),
    ("nbudget_clear", "clear", nbudget_clear),
    ("nbudget_check", "check", nbudget_check),
    ("nbudget_ok", "ok", nbudget_ok),
    ("nbudget_remain", "remain", nbudget_remain),
    ("nbudget_charge", "charge", nbudget_charge),
    ("nbudget_used", "used", nbudget_used),
    ("nbudget_reset_used", "reset_used", nbudget_reset_used),
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

pub const MODULE_NAME: &str = "nbudget";
pub const MODULE_PATHS: &[&str] = &["nbudget", "std/nbudget"];

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
        with_state(|s| {
            s.limits.clear();
            s.used.clear();
        });
    }

    fn obj(pairs: &[(&str, ValueRef)]) -> ValueRef {
        let mut map = HashMap::new();
        for (k, v) in pairs {
            map.insert((*k).to_string(), v.clone());
        }
        Value::Object(map).ref_cell()
    }

    fn i(n: i64) -> ValueRef {
        Value::Int(n).ref_cell()
    }

    fn f(n: f64) -> ValueRef {
        Value::Float(n).ref_cell()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    #[test]
    fn set_get_clear_limits() {
        reset();
        nbudget_set(&[obj(&[("ram_mb", i(1024)), ("usd", f(1.5))])], span()).unwrap();
        let g = nbudget_get(&[], span()).unwrap();
        match &*g.borrow() {
            Value::Object(m) => {
                assert!(matches!(&*m.get("ram_mb").unwrap().borrow(), Value::Int(1024)));
                assert!(matches!(&*m.get("usd").unwrap().borrow(), Value::Float(x) if (*x - 1.5).abs() < 1e-9));
                assert!(m.get("cpu_pct").is_none());
            }
            other => panic!("expected object, got {other:?}"),
        }
        nbudget_clear(&[], span()).unwrap();
        let g2 = nbudget_get(&[], span()).unwrap();
        match &*g2.borrow() {
            Value::Object(m) => assert!(m.is_empty()),
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn charge_check_remain_ok() {
        reset();
        nbudget_set(&[obj(&[("tokens", i(100)), ("usd", f(2.0))])], span()).unwrap();
        nbudget_charge(&[s("tokens"), i(40)], span()).unwrap();
        nbudget_charge(&[s("usd"), f(0.5)], span()).unwrap();

        let ok = nbudget_ok(&[], span()).unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(true)));

        let rem = nbudget_remain(&[], span()).unwrap();
        match &*rem.borrow() {
            Value::Object(m) => {
                assert!(matches!(&*m.get("tokens").unwrap().borrow(), Value::Int(60)));
                assert!(matches!(&*m.get("usd").unwrap().borrow(), Value::Float(x) if (*x - 1.5).abs() < 1e-9));
            }
            other => panic!("expected object, got {other:?}"),
        }

        let chk = nbudget_check(&[obj(&[("tokens", i(70))])], span()).unwrap();
        match &*chk.borrow() {
            Value::Object(m) => {
                assert!(matches!(&*m.get("ok").unwrap().borrow(), Value::Bool(false)));
                match &*m.get("violations").unwrap().borrow() {
                    Value::Array(v) => assert_eq!(v.len(), 1),
                    other => panic!("expected array, got {other:?}"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn charge_exceed_is_catchable_but_recorded() {
        reset();
        nbudget_set(&[obj(&[("ram_mb", i(100))])], span()).unwrap();
        nbudget_charge(&[s("ram_mb"), i(60)], span()).unwrap();
        let over = nbudget_charge(&[s("ram_mb"), i(50)], span()).unwrap();
        assert!(matches!(&*over.borrow(), Value::Error(_)));
        let used = nbudget_used(&[], span()).unwrap();
        match &*used.borrow() {
            Value::Object(m) => {
                assert!(matches!(&*m.get("ram_mb").unwrap().borrow(), Value::Int(110)));
            }
            other => panic!("expected object, got {other:?}"),
        }
        let ok = nbudget_ok(&[], span()).unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(false)));
    }

    #[test]
    fn reset_used_and_unknown_kind() {
        reset();
        nbudget_set(&[obj(&[("cpu_pct", i(50))])], span()).unwrap();
        nbudget_charge(&[s("cpu_pct"), i(10)], span()).unwrap();
        nbudget_reset_used(&[], span()).unwrap();
        let used = nbudget_used(&[], span()).unwrap();
        match &*used.borrow() {
            Value::Object(m) => assert!(m.is_empty()),
            other => panic!("expected object, got {other:?}"),
        }
        let err = nbudget_charge(&[s("disk"), i(1)], span());
        assert!(err.is_err());
    }

    #[test]
    fn arity_and_type_errors() {
        reset();
        assert!(nbudget_get(&[i(1)], span()).is_err());
        assert!(nbudget_set(&[i(1)], span()).is_err());
        assert!(nbudget_charge(&[s("tokens")], span()).is_err());
    }
}
