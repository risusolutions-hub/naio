//! Native nfuzz standard library — deterministic property/fuzz helpers
//! backed by a thread-local xorshift64 RNG.
//!
//! Import with `import "nfuzz"` (or `import "std/nfuzz"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// codes.rs integration pending — local constants until wired.
const E3110_NFUZZ_ARITY: u32 = 3110;
const E3111_NFUZZ_ERROR: u32 = 3111;
const E3112_NFUZZ_TYPE: u32 = 3112;
#[allow(dead_code)]
const E3113_NFUZZ_INVALID_HANDLE: u32 = 3113; // reserved; global RNG only

/// Cap generated buffers / strings / case lists.
const MAX_GEN_LEN: i64 = 16 * 1024 * 1024;

const DEFAULT_STRING_LEN: i64 = 8;
const ALNUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

// ---------------------------------------------------------------------------
// xorshift64
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn seeded(seed: u64) -> Self {
        // Zero state is fixed-point for xorshift; nudge it.
        Self {
            state: if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed },
        }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Uniform float in [0, 1) with 53-bit precision.
    #[inline]
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Unbiased integer in [0, bound).
    fn next_below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        loop {
            let x = self.next_u64();
            let hi = ((x as u128 * bound as u128) >> 64) as u64;
            let lo = x.wrapping_mul(bound);
            if lo >= bound || lo >= bound.wrapping_neg() % bound {
                return hi;
            }
        }
    }

    /// Inclusive integer range [lo, hi].
    fn int_range(&mut self, lo: i64, hi: i64) -> i64 {
        if lo == i64::MIN && hi == i64::MAX {
            return self.next_u64() as i64;
        }
        let width = (hi as i128 - lo as i128 + 1) as u64;
        lo.wrapping_add(self.next_below(width) as i64)
    }
}

thread_local! {
    static RNG: RefCell<XorShift64> = RefCell::new(XorShift64::seeded(0xC0FF_EE42_5EED_F00D));
}

fn with_rng<T>(f: impl FnOnce(&mut XorShift64) -> T) -> T {
    RNG.with(|g| f(&mut g.borrow_mut()))
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3110_NFUZZ_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3110_NFUZZ_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3112_NFUZZ_TYPE, msg.into())
}

fn fuzz_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3111_NFUZZ_ERROR, "nfuzz_error", msg.into(), span)
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

fn checked_len(n: i64, name: &str, span: Span) -> Result<usize, ValueRef> {
    if n < 0 {
        return Err(fuzz_err(span, format!("{name}() length must be >= 0")));
    }
    if n > MAX_GEN_LEN {
        return Err(fuzz_err(span, format!("{name}() length too large")));
    }
    Ok(n as usize)
}

fn array_items(value: &Value, name: &str, span: Span) -> NiaoResult<Vec<ValueRef>> {
    match value {
        Value::Array(items) => Ok(items.clone()),
        Value::IntArray(v) => Ok(v.iter().map(|n| Value::Int(*n).ref_cell()).collect()),
        Value::FloatArray(v) => Ok(v.iter().map(|f| Value::Float(*f).ref_cell()).collect()),
        Value::StringArray(sa) => Ok(sa
            .dense_vec()
            .into_iter()
            .map(|s| Value::String(s).ref_cell())
            .collect()),
        other => Err(type_err(
            span,
            format!("{name}() expects an array, got {}", other.type_name()),
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nfuzz_seed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfuzz_seed", span)?;
    let seed = int_arg(args, 0, "nfuzz_seed", span)?;
    with_rng(|g| *g = XorShift64::seeded(seed as u64));
    Ok(Value::Nil.ref_cell())
}

fn nfuzz_int(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfuzz_int", span)?;
    let lo = int_arg(args, 0, "nfuzz_int", span)?;
    let hi = int_arg(args, 1, "nfuzz_int", span)?;
    if lo > hi {
        return Ok(fuzz_err(span, "nfuzz_int() requires min <= max"));
    }
    Ok(Value::Int(with_rng(|g| g.int_range(lo, hi))).ref_cell())
}

fn nfuzz_float(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfuzz_float", span)?;
    let lo = num_arg(args, 0, "nfuzz_float", span)?;
    let hi = num_arg(args, 1, "nfuzz_float", span)?;
    if lo > hi {
        return Ok(fuzz_err(span, "nfuzz_float() requires min <= max"));
    }
    let x = with_rng(|g| g.next_f64());
    Ok(Value::Float(lo + x * (hi - lo)).ref_cell())
}

fn nfuzz_bool(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nfuzz_bool", span)?;
    Ok(Value::Bool(with_rng(|g| g.next_u64() & 1 == 0)).ref_cell())
}

fn nfuzz_string(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nfuzz_string", span)?;
    let n = if args.is_empty() {
        DEFAULT_STRING_LEN
    } else {
        int_arg(args, 0, "nfuzz_string", span)?
    };
    let n = match checked_len(n, "nfuzz_string", span) {
        Ok(n) => n,
        Err(e) => return Ok(e),
    };
    let mut out = String::with_capacity(n);
    with_rng(|g| {
        for _ in 0..n {
            out.push(ALNUM[g.next_below(ALNUM.len() as u64) as usize] as char);
        }
    });
    Ok(Value::String(out).ref_cell())
}

fn nfuzz_pick(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfuzz_pick", span)?;
    let items = array_items(&args[0].borrow(), "nfuzz_pick", span)?;
    if items.is_empty() {
        return Ok(fuzz_err(span, "nfuzz_pick() on empty array"));
    }
    let idx = with_rng(|g| g.next_below(items.len() as u64)) as usize;
    Ok(Rc::clone(&items[idx]))
}

/// Fisher–Yates on a copy — original array is not modified.
fn nfuzz_shuffle(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfuzz_shuffle", span)?;
    let mut items = array_items(&args[0].borrow(), "nfuzz_shuffle", span)?;
    with_rng(|g| {
        for i in (1..items.len()).rev() {
            let j = g.next_below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    });
    Ok(Value::Array(items).ref_cell())
}

fn nfuzz_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfuzz_bytes", span)?;
    let n = int_arg(args, 0, "nfuzz_bytes", span)?;
    let n = match checked_len(n, "nfuzz_bytes", span) {
        Ok(n) => n,
        Err(e) => return Ok(e),
    };
    let mut out = Vec::with_capacity(n);
    with_rng(|g| {
        while out.len() + 8 <= n {
            out.extend_from_slice(&g.next_u64().to_le_bytes());
        }
        while out.len() < n {
            out.push(g.next_u64() as u8);
        }
    });
    Ok(Value::ByteArray(out).ref_cell())
}

fn nfuzz_cases(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nfuzz_cases", span)?;
    let n = int_arg(args, 0, "nfuzz_cases", span)?;
    let lo = int_arg(args, 1, "nfuzz_cases", span)?;
    let hi = int_arg(args, 2, "nfuzz_cases", span)?;
    if lo > hi {
        return Ok(fuzz_err(span, "nfuzz_cases() requires min <= max"));
    }
    let n = match checked_len(n, "nfuzz_cases", span) {
        Ok(n) => n,
        Err(e) => return Ok(e),
    };
    let mut out = Vec::with_capacity(n);
    with_rng(|g| {
        for _ in 0..n {
            out.push(g.int_range(lo, hi));
        }
    });
    Ok(Value::IntArray(out).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nfuzz_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nfuzz_fns![
    ("nfuzz_seed", "seed", nfuzz_seed),
    ("nfuzz_int", "int", nfuzz_int),
    ("nfuzz_float", "float", nfuzz_float),
    ("nfuzz_bool", "bool", nfuzz_bool),
    ("nfuzz_string", "string", nfuzz_string),
    ("nfuzz_pick", "pick", nfuzz_pick),
    ("nfuzz_shuffle", "shuffle", nfuzz_shuffle),
    ("nfuzz_bytes", "bytes", nfuzz_bytes),
    ("nfuzz_cases", "cases", nfuzz_cases),
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

pub const MODULE_NAME: &str = "nfuzz";
pub const MODULE_PATHS: &[&str] = &["nfuzz", "std/nfuzz"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn f(v: f64) -> ValueRef {
        Value::Float(v).ref_cell()
    }

    #[test]
    fn seeded_is_deterministic() {
        nfuzz_seed(&[i(42)], span()).unwrap();
        let a: Vec<i64> = (0..20)
            .map(|_| match &*nfuzz_int(&[i(0), i(1_000_000)], span()).unwrap().borrow() {
                Value::Int(n) => *n,
                other => panic!("expected int, got {other:?}"),
            })
            .collect();

        nfuzz_seed(&[i(42)], span()).unwrap();
        let b: Vec<i64> = (0..20)
            .map(|_| match &*nfuzz_int(&[i(0), i(1_000_000)], span()).unwrap().borrow() {
                Value::Int(n) => *n,
                other => panic!("expected int, got {other:?}"),
            })
            .collect();
        assert_eq!(a, b);
    }

    #[test]
    fn int_respects_bounds() {
        nfuzz_seed(&[i(7)], span()).unwrap();
        for _ in 0..500 {
            let v = nfuzz_int(&[i(-3), i(3)], span()).unwrap();
            match &*v.borrow() {
                Value::Int(n) => assert!((-3..=3).contains(n)),
                other => panic!("expected int, got {other:?}"),
            }
        }
    }

    #[test]
    fn float_in_range() {
        nfuzz_seed(&[i(9)], span()).unwrap();
        for _ in 0..500 {
            let v = nfuzz_float(&[f(1.0), f(2.0)], span()).unwrap();
            match &*v.borrow() {
                Value::Float(x) => assert!((1.0..2.0).contains(x)),
                other => panic!("expected float, got {other:?}"),
            }
        }
    }

    #[test]
    fn bool_and_string_defaults() {
        nfuzz_seed(&[i(1)], span()).unwrap();
        let b = nfuzz_bool(&[], span()).unwrap();
        assert!(matches!(&*b.borrow(), Value::Bool(_)));

        let s = nfuzz_string(&[], span()).unwrap();
        match &*s.borrow() {
            Value::String(t) => {
                assert_eq!(t.len(), DEFAULT_STRING_LEN as usize);
                assert!(t.chars().all(|c| ALNUM.contains(&(c as u8))));
            }
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn pick_and_shuffle_copy() {
        nfuzz_seed(&[i(3)], span()).unwrap();
        let arr = Value::IntArray(vec![10, 20, 30, 40]).ref_cell();
        let picked = nfuzz_pick(&[arr.clone()], span()).unwrap();
        match &*picked.borrow() {
            Value::Int(n) => assert!([10, 20, 30, 40].contains(n)),
            other => panic!("expected int, got {other:?}"),
        }

        let original = match &*arr.borrow() {
            Value::IntArray(v) => v.clone(),
            other => panic!("expected int array, got {other:?}"),
        };
        let shuffled = nfuzz_shuffle(&[arr.clone()], span()).unwrap();
        // Original unchanged.
        match &*arr.borrow() {
            Value::IntArray(v) => assert_eq!(*v, original),
            other => panic!("expected int array, got {other:?}"),
        }
        match &*shuffled.borrow() {
            Value::Array(items) => {
                assert_eq!(items.len(), 4);
                let mut vals: Vec<i64> = items
                    .iter()
                    .map(|v| match &*v.borrow() {
                        Value::Int(n) => *n,
                        other => panic!("expected int, got {other:?}"),
                    })
                    .collect();
                vals.sort_unstable();
                assert_eq!(vals, vec![10, 20, 30, 40]);
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn bytes_and_cases() {
        nfuzz_seed(&[i(11)], span()).unwrap();
        let bytes = nfuzz_bytes(&[i(16)], span()).unwrap();
        match &*bytes.borrow() {
            Value::ByteArray(b) => assert_eq!(b.len(), 16),
            other => panic!("expected byte array, got {other:?}"),
        }

        let cases = nfuzz_cases(&[i(50), i(0), i(9)], span()).unwrap();
        match &*cases.borrow() {
            Value::IntArray(v) => {
                assert_eq!(v.len(), 50);
                assert!(v.iter().all(|n| (0..=9).contains(n)));
            }
            other => panic!("expected int array, got {other:?}"),
        }
    }

    #[test]
    fn empty_pick_is_error_value() {
        let empty = Value::Array(vec![]).ref_cell();
        let v = nfuzz_pick(&[empty], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn bad_range_is_error_value() {
        let v = nfuzz_int(&[i(5), i(1)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn arity_mismatch() {
        let err = nfuzz_int(&[i(1)], span());
        assert!(err.is_err());
    }
}
