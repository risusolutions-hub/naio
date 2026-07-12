//! Native npar standard library — explicit rayon parallel ops on packed
//! `FloatArray` / `IntArray`, plus `set_threads` for pool sizing.
//!
//! Import with `import "npar"` (or `import "std/npar"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

const E3390_NPAR_ARITY: u32 = 3390;
const E3391_NPAR_ERROR: u32 = 3391;
const E3392_NPAR_TYPE: u32 = 3392;

const CHUNK: usize = 4096;

// ---------------------------------------------------------------------------
// Thread pool (custom pool when set_threads is used)
// ---------------------------------------------------------------------------

thread_local! {
    static THREAD_COUNT: RefCell<usize> = const { RefCell::new(0) };
}

static CUSTOM_POOL: Mutex<Option<ThreadPool>> = Mutex::new(None);

fn install<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    let pool = CUSTOM_POOL.lock().unwrap();
    if let Some(p) = pool.as_ref() {
        p.install(f)
    } else {
        f()
    }
}

fn active_threads() -> usize {
    let custom = CUSTOM_POOL.lock().unwrap();
    if let Some(p) = custom.as_ref() {
        return p.current_num_threads();
    }
    rayon::current_num_threads()
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3390_NPAR_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3392_NPAR_TYPE, msg.into())
}

fn par_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3391_NPAR_ERROR, "npar_error", msg.into(), span)
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

// ---------------------------------------------------------------------------
// Parallel kernels
// ---------------------------------------------------------------------------

#[inline]
fn par_sum_i64(v: &[i64]) -> i64 {
    install(|| v.par_chunks(CHUNK).map(|c| c.iter().map(|&x| x as i128).sum::<i128>() as i64).sum())
}

#[inline]
fn par_sum_f64(v: &[f64]) -> f64 {
    install(|| v.par_chunks(CHUNK).map(|c| c.iter().sum::<f64>()).sum())
}

#[inline]
fn par_add_i64(a: &[i64], b: &[i64]) -> Vec<i64> {
    install(|| {
        a.par_iter()
            .zip(b.par_iter())
            .map(|(&x, &y)| x.wrapping_add(y))
            .collect()
    })
}

#[inline]
fn par_add_f64(a: &[f64], b: &[f64]) -> Vec<f64> {
    install(|| a.par_iter().zip(b.par_iter()).map(|(&x, &y)| x + y).collect())
}

#[inline]
fn par_mul_i64(a: &[i64], b: &[i64]) -> Vec<i64> {
    install(|| {
        a.par_iter()
            .zip(b.par_iter())
            .map(|(&x, &y)| x.wrapping_mul(y))
            .collect()
    })
}

#[inline]
fn par_mul_f64(a: &[f64], b: &[f64]) -> Vec<f64> {
    install(|| a.par_iter().zip(b.par_iter()).map(|(&x, &y)| x * y).collect())
}

#[inline]
fn par_dot_i64(a: &[i64], b: &[i64]) -> i64 {
    install(|| {
        a.par_chunks(CHUNK)
            .zip(b.par_chunks(CHUNK))
            .map(|(ca, cb)| {
                ca.iter()
                    .zip(cb.iter())
                    .map(|(&x, &y)| x as i128 * y as i128)
                    .sum::<i128>() as i64
            })
            .sum()
    })
}

#[inline]
fn par_dot_f64(a: &[f64], b: &[f64]) -> f64 {
    install(|| {
        a.par_chunks(CHUNK)
            .zip(b.par_chunks(CHUNK))
            .map(|(ca, cb)| ca.iter().zip(cb.iter()).map(|(&x, &y)| x * y).sum::<f64>())
            .sum()
    })
}

fn map_int_op(op: &str, x: i64) -> Result<i64, String> {
    match op {
        "id" => Ok(x),
        "neg" => Ok(x.wrapping_neg()),
        "abs" => Ok(x.saturating_abs()),
        "double" => Ok(x.wrapping_mul(2)),
        other => Err(format!("unknown npar map op '{other}'")),
    }
}

fn map_float_op(op: &str, x: f64) -> Result<f64, String> {
    match op {
        "id" => Ok(x),
        "neg" => Ok(-x),
        "abs" => Ok(x.abs()),
        "square" => Ok(x * x),
        "sqrt" => Ok(x.sqrt()),
        other => Err(format!("unknown npar map op '{other}'")),
    }
}

#[inline]
fn par_map_i64(v: &[i64], op: &str) -> Result<Vec<i64>, String> {
    install(|| {
        v.par_iter()
            .map(|&x| map_int_op(op, x))
            .collect::<Result<Vec<_>, _>>()
    })
}

#[inline]
fn par_map_f64(v: &[f64], op: &str) -> Result<Vec<f64>, String> {
    install(|| {
        v.par_iter()
            .map(|&x| map_float_op(op, x))
            .collect::<Result<Vec<_>, _>>()
    })
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn npar_set_threads(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npar_set_threads", span)?;
    let n = int_arg(args, 0, "npar_set_threads", span)? as usize;
    if n == 0 {
        return Ok(par_err(
            span,
            "npar_set_threads() expects a positive thread count",
        ));
    }
    let pool = ThreadPoolBuilder::new()
        .num_threads(n)
        .build()
        .map_err(|e| {
            RuntimeError::at(
                span,
                E3391_NPAR_ERROR,
                format!("npar_set_threads() failed: {e}"),
            )
        })?;
    {
        let mut guard = CUSTOM_POOL.lock().unwrap();
        *guard = Some(pool);
    }
    THREAD_COUNT.with(|c| *c.borrow_mut() = n);
    Ok(Value::Int(n as i64).ref_cell())
}

fn npar_threads(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "npar_threads", span)?;
    Ok(Value::Int(active_threads() as i64).ref_cell())
}

fn npar_sum(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npar_sum", span)?;
    match &*args[0].borrow() {
        Value::IntArray(v) => Ok(Value::Int(par_sum_i64(v)).ref_cell()),
        Value::FloatArray(v) => Ok(Value::Float(par_sum_f64(v)).ref_cell()),
        other => Err(type_err(
            span,
            format!(
                "npar_sum() expects IntArray or FloatArray, got {}",
                other.type_name()
            ),
        )),
    }
}

fn npar_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "npar_add", span)?;
    let a = args[0].borrow();
    let b = args[1].borrow();
    match (&*a, &*b) {
        (Value::IntArray(x), Value::IntArray(y)) if x.len() == y.len() => {
            Ok(Value::IntArray(par_add_i64(x, y)).ref_cell())
        }
        (Value::FloatArray(x), Value::FloatArray(y)) if x.len() == y.len() => {
            Ok(Value::FloatArray(par_add_f64(x, y)).ref_cell())
        }
        (Value::IntArray(x), Value::IntArray(y)) => Ok(par_err(
            span,
            format!("npar_add() length mismatch: {} vs {}", x.len(), y.len()),
        )),
        _ => Ok(par_err(
            span,
            "npar_add() expects two IntArrays or two FloatArrays of equal length",
        )),
    }
}

fn npar_mul(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "npar_mul", span)?;
    let a = args[0].borrow();
    let b = args[1].borrow();
    match (&*a, &*b) {
        (Value::IntArray(x), Value::IntArray(y)) if x.len() == y.len() => {
            Ok(Value::IntArray(par_mul_i64(x, y)).ref_cell())
        }
        (Value::FloatArray(x), Value::FloatArray(y)) if x.len() == y.len() => {
            Ok(Value::FloatArray(par_mul_f64(x, y)).ref_cell())
        }
        (Value::IntArray(x), Value::IntArray(y)) => Ok(par_err(
            span,
            format!("npar_mul() length mismatch: {} vs {}", x.len(), y.len()),
        )),
        _ => Ok(par_err(
            span,
            "npar_mul() expects two IntArrays or two FloatArrays of equal length",
        )),
    }
}

fn npar_dot(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "npar_dot", span)?;
    let a = args[0].borrow();
    let b = args[1].borrow();
    match (&*a, &*b) {
        (Value::IntArray(x), Value::IntArray(y)) if x.len() == y.len() => {
            Ok(Value::Int(par_dot_i64(x, y)).ref_cell())
        }
        (Value::FloatArray(x), Value::FloatArray(y)) if x.len() == y.len() => {
            Ok(Value::Float(par_dot_f64(x, y)).ref_cell())
        }
        (Value::IntArray(x), Value::IntArray(y)) => Ok(par_err(
            span,
            format!("npar_dot() length mismatch: {} vs {}", x.len(), y.len()),
        )),
        _ => Ok(par_err(
            span,
            "npar_dot() expects two IntArrays or two FloatArrays of equal length",
        )),
    }
}

fn npar_map(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "npar_map", span)?;
    let op = string_arg(args, 1, "npar_map", span)?;
    match &*args[0].borrow() {
        Value::IntArray(v) => match par_map_i64(v, &op) {
            Ok(out) => Ok(Value::IntArray(out).ref_cell()),
            Err(msg) => Ok(par_err(span, msg)),
        },
        Value::FloatArray(v) => match par_map_f64(v, &op) {
            Ok(out) => Ok(Value::FloatArray(out).ref_cell()),
            Err(msg) => Ok(par_err(span, msg)),
        },
        other => Err(type_err(
            span,
            format!(
                "npar_map() expects IntArray or FloatArray as argument 1, got {}",
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! npar_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

npar_fns![
    ("npar_set_threads", "set_threads", npar_set_threads),
    ("npar_threads", "threads", npar_threads),
    ("npar_sum", "sum", npar_sum),
    ("npar_add", "add", npar_add),
    ("npar_mul", "mul", npar_mul),
    ("npar_dot", "dot", npar_dot),
    ("npar_map", "map", npar_map),
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

pub const MODULE_NAME: &str = "npar";
pub const MODULE_PATHS: &[&str] = &["npar", "std/npar"];

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

    fn ia(v: Vec<i64>) -> ValueRef {
        Value::IntArray(v).ref_cell()
    }

    fn fa(v: Vec<f64>) -> ValueRef {
        Value::FloatArray(v).ref_cell()
    }

    fn expect_int(r: NiaoResult<ValueRef>) -> i64 {
        match &*r.unwrap().borrow() {
            Value::Int(n) => *n,
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[test]
    fn parallel_sum_and_add() {
        let data: Vec<i64> = (0..10_000).collect();
        assert_eq!(expect_int(npar_sum(&[ia(data.clone())], span())), 49_995_000);
        let ones = vec![1_i64; 1000];
        let out = npar_add(&[ia(data[..1000].to_vec()), ia(ones)], span()).unwrap();
        match &*out.borrow() {
            Value::IntArray(v) => assert_eq!(v.len(), 1000),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn map_ops() {
        let v = ia(vec![-2, 0, 3, 4]);
        let out = npar_map(&[v, Value::String("abs".into()).ref_cell()], span()).unwrap();
        match &*out.borrow() {
            Value::IntArray(a) => assert_eq!(a, &vec![2, 0, 3, 4]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn set_threads_positive() {
        let before = expect_int(npar_threads(&[], span()));
        assert!(before >= 1);
        assert_eq!(expect_int(npar_set_threads(&[Value::Int(2).ref_cell()], span())), 2);
        assert_eq!(expect_int(npar_threads(&[], span())), 2);
    }

    #[test]
    fn dot_f64() {
        let a = fa(vec![1.0, 2.0, 3.0]);
        let b = fa(vec![4.0, 5.0, 6.0]);
        match &*npar_dot(&[a, b], span()).unwrap().borrow() {
            Value::Float(x) => assert!((*x - 32.0).abs() < 1e-12),
            other => panic!("{other:?}"),
        }
    }
}
