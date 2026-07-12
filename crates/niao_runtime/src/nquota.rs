//! Native nquota standard library — token-bucket rate limiting with
//! ncache-style integer handles. Refill is based on `SystemTime` elapsed
//! since the last update (no background threads).
//!
//! Import with `import "nquota"` (or `import "std/nquota"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::SystemTime;

// Wired in codes.rs by central integration.
const E3090_NQUOTA_ARITY: u32 = 3090;
const E3091_NQUOTA_ERROR: u32 = 3091;
const E3092_NQUOTA_TYPE: u32 = 3092;
const E3093_NQUOTA_INVALID_HANDLE: u32 = 3093;

// ---------------------------------------------------------------------------
// Token bucket
// ---------------------------------------------------------------------------

struct Bucket {
    rate: f64,
    burst: f64,
    tokens: f64,
    last: SystemTime,
}

impl Bucket {
    fn new(rate: f64, burst: f64) -> Self {
        let now = SystemTime::now();
        Bucket {
            rate,
            burst,
            tokens: burst,
            last: now,
        }
    }

    fn refill(&mut self) {
        let now = SystemTime::now();
        if let Ok(elapsed) = now.duration_since(self.last) {
            let add = self.rate * elapsed.as_secs_f64();
            if add > 0.0 {
                self.tokens = (self.tokens + add).min(self.burst);
                self.last = now;
            }
        } else {
            // Clock went backwards — just reset the watermark.
            self.last = now;
        }
    }

    fn take(&mut self, n: f64) -> bool {
        self.refill();
        if self.tokens + f64::EPSILON >= n {
            self.tokens = (self.tokens - n).max(0.0);
            true
        } else {
            false
        }
    }

    fn ok(&mut self) -> bool {
        self.refill();
        self.tokens + f64::EPSILON >= 1.0
    }

    fn wait_ms(&mut self) -> i64 {
        self.refill();
        if self.tokens + f64::EPSILON >= 1.0 {
            return 0;
        }
        if self.rate <= 0.0 {
            return i64::MAX;
        }
        let need = 1.0 - self.tokens;
        let secs = need / self.rate;
        let ms = (secs * 1000.0).ceil();
        if !ms.is_finite() || ms >= i64::MAX as f64 {
            i64::MAX
        } else {
            ms.max(0.0) as i64
        }
    }

    fn reset(&mut self) {
        self.tokens = self.burst;
        self.last = SystemTime::now();
    }
}

thread_local! {
    static BUCKETS: RefCell<HashMap<i64, Bucket>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn with_bucket<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Bucket) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    BUCKETS.with(|buckets| {
        let mut buckets = buckets.borrow_mut();
        match buckets.get_mut(&id) {
            Some(b) => Ok(Ok(f(b))),
            None => Ok(Err(error_value(
                E3093_NQUOTA_INVALID_HANDLE,
                "nquota_error",
                format!("invalid or closed quota handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3090_NQUOTA_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3090_NQUOTA_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3092_NQUOTA_TYPE, msg.into())
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

fn num_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
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

fn nquota_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3091_NQUOTA_ERROR, "nquota_error", msg.into(), span)
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nquota_new(rate_per_sec, burst?) → handle
fn nquota_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nquota_new", span)?;
    let rate = num_arg(args, 0, "nquota_new", span)?;
    if !(rate.is_finite() && rate > 0.0) {
        return Ok(nquota_err(
            span,
            "nquota_new() rate_per_sec must be a finite number > 0",
        ));
    }
    let burst = if args.len() > 1 {
        let b = num_arg(args, 1, "nquota_new", span)?;
        if !(b.is_finite() && b > 0.0) {
            return Ok(nquota_err(
                span,
                "nquota_new() burst must be a finite number > 0",
            ));
        }
        b
    } else {
        rate.max(1.0)
    };
    let id = new_handle();
    BUCKETS.with(|buckets| {
        buckets.borrow_mut().insert(id, Bucket::new(rate, burst));
    });
    Ok(Value::Int(id).ref_cell())
}

/// nquota_take(handle, n?) → bool
fn nquota_take(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nquota_take", span)?;
    let id = int_arg(args, 0, "nquota_take", span)?;
    let n = if args.len() > 1 {
        let v = num_arg(args, 1, "nquota_take", span)?;
        if !(v.is_finite() && v > 0.0) {
            return Ok(nquota_err(span, "nquota_take() n must be a finite number > 0"));
        }
        v
    } else {
        1.0
    };
    match with_bucket(id, span, |b| b.take(n))? {
        Ok(ok) => Ok(Value::Bool(ok).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nquota_ok(handle) → bool — true when at least 1 token is available.
fn nquota_ok(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nquota_ok", span)?;
    let id = int_arg(args, 0, "nquota_ok", span)?;
    match with_bucket(id, span, |b| b.ok())? {
        Ok(ok) => Ok(Value::Bool(ok).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nquota_wait_ms(handle) → suggested milliseconds until 1 token is available.
fn nquota_wait_ms(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nquota_wait_ms", span)?;
    let id = int_arg(args, 0, "nquota_wait_ms", span)?;
    match with_bucket(id, span, |b| b.wait_ms())? {
        Ok(ms) => Ok(Value::Int(ms).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nquota_reset(handle) — refill to full burst.
fn nquota_reset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nquota_reset", span)?;
    let id = int_arg(args, 0, "nquota_reset", span)?;
    match with_bucket(id, span, |b| b.reset())? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nquota_stats(handle) → {tokens, rate, burst}
fn nquota_stats(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nquota_stats", span)?;
    let id = int_arg(args, 0, "nquota_stats", span)?;
    match with_bucket(id, span, |b| {
        b.refill();
        (b.tokens, b.rate, b.burst)
    })? {
        Ok((tokens, rate, burst)) => {
            let mut map = HashMap::new();
            map.insert("tokens".to_string(), Value::Float(tokens).ref_cell());
            map.insert("rate".to_string(), Value::Float(rate).ref_cell());
            map.insert("burst".to_string(), Value::Float(burst).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

fn nquota_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nquota_close", span)?;
    let id = int_arg(args, 0, "nquota_close", span)?;
    let removed = BUCKETS.with(|buckets| buckets.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nquota_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nquota_fns![
    ("nquota_new", "new", nquota_new),
    ("nquota_take", "take", nquota_take),
    ("nquota_ok", "ok", nquota_ok),
    ("nquota_wait_ms", "wait_ms", nquota_wait_ms),
    ("nquota_reset", "reset", nquota_reset),
    ("nquota_stats", "stats", nquota_stats),
    ("nquota_close", "close", nquota_close),
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

pub const MODULE_NAME: &str = "nquota";
pub const MODULE_PATHS: &[&str] = &["nquota", "std/nquota"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;
    use std::time::Duration;

    fn span() -> Span {
        Span::dummy()
    }

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn f(v: f64) -> ValueRef {
        Value::Float(v).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> ValueRef {
        let v = r.unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(_)), "expected handle int");
        v
    }

    fn as_bool(r: NiaoResult<ValueRef>) -> bool {
        match &*r.unwrap().borrow() {
            Value::Bool(b) => *b,
            other => panic!("expected bool, got {other:?}"),
        }
    }

    fn as_int(r: NiaoResult<ValueRef>) -> i64 {
        match &*r.unwrap().borrow() {
            Value::Int(n) => *n,
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[test]
    fn take_consumes_tokens() {
        let h = handle(nquota_new(&[i(10), i(2)], span()));
        assert!(as_bool(nquota_take(&[h.clone()], span())));
        assert!(as_bool(nquota_take(&[h.clone()], span())));
        assert!(!as_bool(nquota_take(&[h.clone()], span())));
        assert!(!as_bool(nquota_ok(&[h.clone()], span())));
        nquota_close(&[h], span()).unwrap();
    }

    #[test]
    fn take_n_and_reset() {
        let h = handle(nquota_new(&[i(100), i(5)], span()));
        assert!(as_bool(nquota_take(&[h.clone(), i(3)], span())));
        let stats = nquota_stats(&[h.clone()], span()).unwrap();
        match &*stats.borrow() {
            Value::Object(map) => {
                let tokens = match &*map.get("tokens").unwrap().borrow() {
                    Value::Float(t) => *t,
                    other => panic!("expected float tokens, got {other:?}"),
                };
                assert!((tokens - 2.0).abs() < 1e-6);
            }
            other => panic!("expected object, got {other:?}"),
        }
        nquota_reset(&[h.clone()], span()).unwrap();
        assert!(as_bool(nquota_ok(&[h.clone()], span())));
        let stats2 = nquota_stats(&[h.clone()], span()).unwrap();
        match &*stats2.borrow() {
            Value::Object(map) => {
                let tokens = match &*map.get("tokens").unwrap().borrow() {
                    Value::Float(t) => *t,
                    other => panic!("expected float tokens, got {other:?}"),
                };
                assert!((tokens - 5.0).abs() < 1e-6);
                assert!(matches!(&*map.get("rate").unwrap().borrow(), Value::Float(r) if (*r - 100.0).abs() < 1e-9));
                assert!(matches!(&*map.get("burst").unwrap().borrow(), Value::Float(b) if (*b - 5.0).abs() < 1e-9));
            }
            other => panic!("expected object, got {other:?}"),
        }
        nquota_close(&[h], span()).unwrap();
    }

    #[test]
    fn wait_ms_when_empty() {
        let h = handle(nquota_new(&[i(10), i(1)], span()));
        assert!(as_bool(nquota_take(&[h.clone()], span())));
        let ms = as_int(nquota_wait_ms(&[h.clone()], span()));
        // Need ~100ms for 1 token at 10/s; allow some slack.
        assert!(ms >= 50 && ms <= 200, "wait_ms={ms}");
        // wait_ms does not grant tokens; still waiting.
        let ms2 = as_int(nquota_wait_ms(&[h.clone()], span()));
        assert!(ms2 > 0 && ms2 <= 200, "wait_ms={ms2}");
        nquota_close(&[h], span()).unwrap();
    }

    #[test]
    fn refill_after_elapsed() {
        // 5 tokens/sec → ~200ms for one token; sleep well past that.
        let h = handle(nquota_new(&[f(5.0), i(1)], span()));
        assert!(as_bool(nquota_take(&[h.clone()], span())));
        assert!(!as_bool(nquota_ok(&[h.clone()], span())));
        std::thread::sleep(Duration::from_millis(250));
        assert!(as_bool(nquota_ok(&[h.clone()], span())));
        nquota_close(&[h], span()).unwrap();
    }

    #[test]
    fn default_burst_is_rate() {
        let h = handle(nquota_new(&[i(3)], span()));
        let stats = nquota_stats(&[h.clone()], span()).unwrap();
        match &*stats.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map.get("burst").unwrap().borrow(), Value::Float(b) if (*b - 3.0).abs() < 1e-9));
            }
            other => panic!("expected object, got {other:?}"),
        }
        nquota_close(&[h], span()).unwrap();
    }

    #[test]
    fn invalid_handle_error_value() {
        let v = nquota_take(&[i(424_242)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
        let v = nquota_ok(&[i(424_242)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn close_rejects_reuse() {
        let h = handle(nquota_new(&[i(1), i(1)], span()));
        assert!(as_bool(nquota_close(&[h.clone()], span())));
        let v = nquota_take(&[h], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn bad_rate_is_error_value() {
        let v = nquota_new(&[i(0)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
        let v = nquota_new(&[i(-1), i(1)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn arity_and_type_errors() {
        assert_eq!(
            nquota_new(&[], span()).unwrap_err().code(),
            E3090_NQUOTA_ARITY
        );
        assert_eq!(
            nquota_take(&[Value::String("x".into()).ref_cell()], span())
                .unwrap_err()
                .code(),
            E3092_NQUOTA_TYPE
        );
    }
}
