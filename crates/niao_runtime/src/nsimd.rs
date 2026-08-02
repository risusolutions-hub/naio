//! Native nsimd standard library — unrolled autovectorized f64/i64 kernels
//! on packed `FloatArray` / `IntArray` (`chunks_exact(8)` hot loops).
//!
//! Import with `import "nsimd"` (or `import "std/nsimd"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::rc::Rc;

const E3350_NSIMD_ARITY: u32 = 3350;
const E3351_NSIMD_ERROR: u32 = 3351;
const E3352_NSIMD_TYPE: u32 = 3352;

const UNROLL: usize = 8;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3350_NSIMD_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3352_NSIMD_TYPE, msg.into())
}

fn simd_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3351_NSIMD_ERROR, "nsimd_error", msg.into(), span)
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

fn float_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    match &*args[idx].borrow() {
        Value::Float(x) => Ok(*x),
        Value::Int(n) => Ok(*n as f64),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a float as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn paired_arrays<'a>(
    a: &'a Value,
    b: &'a Value,
    name: &str,
    span: Span,
) -> Result<(&'a [i64], &'a [i64]), ValueRef> {
    match (a, b) {
        (Value::IntArray(x), Value::IntArray(y)) if x.len() == y.len() => Ok((x, y)),
        (Value::IntArray(x), Value::IntArray(y)) => Err(simd_err(
            span,
            format!("{name}() length mismatch: {} vs {}", x.len(), y.len()),
        )),
        _ => Err(simd_err(
            span,
            format!("{name}() expects two IntArrays of equal length"),
        )),
    }
}

fn paired_float_arrays<'a>(
    a: &'a Value,
    b: &'a Value,
    name: &str,
    span: Span,
) -> Result<(&'a [f64], &'a [f64]), ValueRef> {
    match (a, b) {
        (Value::FloatArray(x), Value::FloatArray(y)) if x.len() == y.len() => Ok((x, y)),
        (Value::FloatArray(x), Value::FloatArray(y)) => Err(simd_err(
            span,
            format!("{name}() length mismatch: {} vs {}", x.len(), y.len()),
        )),
        _ => Err(simd_err(
            span,
            format!("{name}() expects two FloatArrays of equal length"),
        )),
    }
}

// ---------------------------------------------------------------------------
// Unrolled kernels (chunks_exact(8) for LLVM autovectorization)
// ---------------------------------------------------------------------------

#[inline]
fn sum_i64_kernel(slice: &[i64]) -> i64 {
    if slice.is_empty() {
        return 0;
    }
    let mut a0: i128 = 0;
    let mut a1: i128 = 0;
    let mut a2: i128 = 0;
    let mut a3: i128 = 0;
    let chunks = slice.chunks_exact(UNROLL);
    let rem = chunks.remainder();
    for chunk in chunks {
        let [c0, c1, c2, c3, c4, c5, c6, c7] = [
            chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
        ];
        a0 += c0 as i128 + c1 as i128;
        a1 += c2 as i128 + c3 as i128;
        a2 += c4 as i128 + c5 as i128;
        a3 += c6 as i128 + c7 as i128;
    }
    let mut acc = a0 + a1 + a2 + a3;
    for &v in rem {
        acc += v as i128;
    }
    acc as i64
}

#[inline]
fn sum_f64_kernel(slice: &[f64]) -> f64 {
    if slice.is_empty() {
        return 0.0;
    }
    let mut a0 = 0.0;
    let mut a1 = 0.0;
    let mut a2 = 0.0;
    let mut a3 = 0.0;
    let chunks = slice.chunks_exact(UNROLL);
    let rem = chunks.remainder();
    for chunk in chunks {
        let [c0, c1, c2, c3, c4, c5, c6, c7] = [
            chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
        ];
        a0 += c0 + c1;
        a1 += c2 + c3;
        a2 += c4 + c5;
        a3 += c6 + c7;
    }
    let mut acc = a0 + a1 + a2 + a3;
    for &v in rem {
        acc += v;
    }
    acc
}

#[inline]
fn dot_i64_kernel(a: &[i64], b: &[i64]) -> i64 {
    debug_assert_eq!(a.len(), b.len());
    if a.is_empty() {
        return 0;
    }
    let mut a0: i128 = 0;
    let mut a1: i128 = 0;
    let mut a2: i128 = 0;
    let mut a3: i128 = 0;
    let a_chunks = a.chunks_exact(UNROLL);
    let b_chunks = b.chunks_exact(UNROLL);
    let a_rem = a_chunks.remainder();
    let b_rem = b_chunks.remainder();
    for (ac, bc) in a_chunks.zip(b_chunks) {
        let [x0, x1, x2, x3, x4, x5, x6, x7] =
            [ac[0], ac[1], ac[2], ac[3], ac[4], ac[5], ac[6], ac[7]];
        let [y0, y1, y2, y3, y4, y5, y6, y7] =
            [bc[0], bc[1], bc[2], bc[3], bc[4], bc[5], bc[6], bc[7]];
        a0 += x0 as i128 * y0 as i128 + x1 as i128 * y1 as i128;
        a1 += x2 as i128 * y2 as i128 + x3 as i128 * y3 as i128;
        a2 += x4 as i128 * y4 as i128 + x5 as i128 * y5 as i128;
        a3 += x6 as i128 * y6 as i128 + x7 as i128 * y7 as i128;
    }
    let mut acc = a0 + a1 + a2 + a3;
    for i in 0..a_rem.len() {
        acc += a_rem[i] as i128 * b_rem[i] as i128;
    }
    acc as i64
}

#[inline]
fn dot_f64_kernel(a: &[f64], b: &[f64]) -> f64 {
    debug_assert_eq!(a.len(), b.len());
    if a.is_empty() {
        return 0.0;
    }
    let mut a0 = 0.0;
    let mut a1 = 0.0;
    let mut a2 = 0.0;
    let mut a3 = 0.0;
    let a_chunks = a.chunks_exact(UNROLL);
    let b_chunks = b.chunks_exact(UNROLL);
    let a_rem = a_chunks.remainder();
    let b_rem = b_chunks.remainder();
    for (ac, bc) in a_chunks.zip(b_chunks) {
        let [x0, x1, x2, x3, x4, x5, x6, x7] =
            [ac[0], ac[1], ac[2], ac[3], ac[4], ac[5], ac[6], ac[7]];
        let [y0, y1, y2, y3, y4, y5, y6, y7] =
            [bc[0], bc[1], bc[2], bc[3], bc[4], bc[5], bc[6], bc[7]];
        a0 += x0 * y0 + x1 * y1;
        a1 += x2 * y2 + x3 * y3;
        a2 += x4 * y4 + x5 * y5;
        a3 += x6 * y6 + x7 * y7;
    }
    let mut acc = a0 + a1 + a2 + a3;
    for i in 0..a_rem.len() {
        acc += a_rem[i] * b_rem[i];
    }
    acc
}

#[inline]
fn zip_binop_i64<F>(a: &[i64], b: &[i64], mut op: F) -> Vec<i64>
where
    F: FnMut(i64, i64) -> i64,
{
    debug_assert_eq!(a.len(), b.len());
    let mut out = Vec::with_capacity(a.len());
    let len = a.len();
    let mut i = 0;
    while i + UNROLL <= len {
        for j in 0..UNROLL {
            out.push(op(a[i + j], b[i + j]));
        }
        i += UNROLL;
    }
    while i < len {
        out.push(op(a[i], b[i]));
        i += 1;
    }
    out
}

#[inline]
fn zip_binop_f64<F>(a: &[f64], b: &[f64], mut op: F) -> Vec<f64>
where
    F: FnMut(f64, f64) -> f64,
{
    debug_assert_eq!(a.len(), b.len());
    let mut out = Vec::with_capacity(a.len());
    let len = a.len();
    let mut i = 0;
    while i + UNROLL <= len {
        for j in 0..UNROLL {
            out.push(op(a[i + j], b[i + j]));
        }
        i += UNROLL;
    }
    while i < len {
        out.push(op(a[i], b[i]));
        i += 1;
    }
    out
}

#[inline]
fn map_unary_i64<F>(a: &[i64], mut op: F) -> Vec<i64>
where
    F: FnMut(i64) -> i64,
{
    let mut out = Vec::with_capacity(a.len());
    let len = a.len();
    let mut i = 0;
    while i + UNROLL <= len {
        for j in 0..UNROLL {
            out.push(op(a[i + j]));
        }
        i += UNROLL;
    }
    while i < len {
        out.push(op(a[i]));
        i += 1;
    }
    out
}

#[inline]
fn map_unary_f64<F>(a: &[f64], mut op: F) -> Vec<f64>
where
    F: FnMut(f64) -> f64,
{
    let mut out = Vec::with_capacity(a.len());
    let len = a.len();
    let mut i = 0;
    while i + UNROLL <= len {
        for j in 0..UNROLL {
            out.push(op(a[i + j]));
        }
        i += UNROLL;
    }
    while i < len {
        out.push(op(a[i]));
        i += 1;
    }
    out
}

#[inline]
fn min_i64_kernel(slice: &[i64]) -> Option<i64> {
    slice.iter().copied().min()
}

#[inline]
fn max_i64_kernel(slice: &[i64]) -> Option<i64> {
    slice.iter().copied().max()
}

#[inline]
fn min_f64_kernel(slice: &[f64]) -> Option<f64> {
    slice
        .iter()
        .copied()
        .min_by(|a, b| a.partial_cmp(b).unwrap())
}

#[inline]
fn max_f64_kernel(slice: &[f64]) -> Option<f64> {
    slice
        .iter()
        .copied()
        .max_by(|a, b| a.partial_cmp(b).unwrap())
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nsimd_sum(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsimd_sum", span)?;
    match &*args[0].borrow() {
        Value::IntArray(v) => Ok(Value::Int(sum_i64_kernel(v)).ref_cell()),
        Value::FloatArray(v) => Ok(Value::Float(sum_f64_kernel(v)).ref_cell()),
        other => Err(type_err(
            span,
            format!(
                "nsimd_sum() expects IntArray or FloatArray, got {}",
                other.type_name()
            ),
        )),
    }
}

fn nsimd_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsimd_add", span)?;
    let a = args[0].borrow();
    let b = args[1].borrow();
    if let Ok((x, y)) = paired_arrays(&a, &b, "nsimd_add", span) {
        return Ok(Value::IntArray(zip_binop_i64(x, y, |a, b| a.wrapping_add(b))).ref_cell());
    }
    if let Ok((x, y)) = paired_float_arrays(&a, &b, "nsimd_add", span) {
        return Ok(Value::FloatArray(zip_binop_f64(x, y, |a, b| a + b)).ref_cell());
    }
    Ok(simd_err(
        span,
        "nsimd_add() expects two IntArrays or two FloatArrays of equal length",
    ))
}

fn nsimd_sub(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsimd_sub", span)?;
    let a = args[0].borrow();
    let b = args[1].borrow();
    if let Ok((x, y)) = paired_arrays(&a, &b, "nsimd_sub", span) {
        return Ok(Value::IntArray(zip_binop_i64(x, y, |a, b| a.wrapping_sub(b))).ref_cell());
    }
    if let Ok((x, y)) = paired_float_arrays(&a, &b, "nsimd_sub", span) {
        return Ok(Value::FloatArray(zip_binop_f64(x, y, |a, b| a - b)).ref_cell());
    }
    Ok(simd_err(
        span,
        "nsimd_sub() expects two IntArrays or two FloatArrays of equal length",
    ))
}

fn nsimd_mul(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsimd_mul", span)?;
    let a = args[0].borrow();
    let b = args[1].borrow();
    if let Ok((x, y)) = paired_arrays(&a, &b, "nsimd_mul", span) {
        return Ok(Value::IntArray(zip_binop_i64(x, y, |a, b| a.wrapping_mul(b))).ref_cell());
    }
    if let Ok((x, y)) = paired_float_arrays(&a, &b, "nsimd_mul", span) {
        return Ok(Value::FloatArray(zip_binop_f64(x, y, |a, b| a * b)).ref_cell());
    }
    Ok(simd_err(
        span,
        "nsimd_mul() expects two IntArrays or two FloatArrays of equal length",
    ))
}

fn nsimd_dot(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsimd_dot", span)?;
    let a = args[0].borrow();
    let b = args[1].borrow();
    if let Ok((x, y)) = paired_arrays(&a, &b, "nsimd_dot", span) {
        return Ok(Value::Int(dot_i64_kernel(x, y)).ref_cell());
    }
    if let Ok((x, y)) = paired_float_arrays(&a, &b, "nsimd_dot", span) {
        return Ok(Value::Float(dot_f64_kernel(x, y)).ref_cell());
    }
    Ok(simd_err(
        span,
        "nsimd_dot() expects two IntArrays or two FloatArrays of equal length",
    ))
}

fn nsimd_scale(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsimd_scale", span)?;
    match &*args[0].borrow() {
        Value::IntArray(v) => {
            let k = int_arg(args, 1, "nsimd_scale", span)?;
            Ok(Value::IntArray(map_unary_i64(v, |x| x.wrapping_mul(k))).ref_cell())
        }
        Value::FloatArray(v) => {
            let k = float_arg(args, 1, "nsimd_scale", span)?;
            Ok(Value::FloatArray(map_unary_f64(v, |x| x * k)).ref_cell())
        }
        other => Err(type_err(
            span,
            format!(
                "nsimd_scale() expects IntArray or FloatArray as argument 1, got {}",
                other.type_name()
            ),
        )),
    }
}

fn nsimd_abs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsimd_abs", span)?;
    match &*args[0].borrow() {
        Value::IntArray(v) => {
            Ok(Value::IntArray(map_unary_i64(v, |x| x.saturating_abs())).ref_cell())
        }
        Value::FloatArray(v) => Ok(Value::FloatArray(map_unary_f64(v, |x| x.abs())).ref_cell()),
        other => Err(type_err(
            span,
            format!(
                "nsimd_abs() expects IntArray or FloatArray, got {}",
                other.type_name()
            ),
        )),
    }
}

fn nsimd_min(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsimd_min", span)?;
    match &*args[0].borrow() {
        Value::IntArray(v) => match min_i64_kernel(v) {
            Some(n) => Ok(Value::Int(n).ref_cell()),
            None => Ok(Value::Nil.ref_cell()),
        },
        Value::FloatArray(v) => match min_f64_kernel(v) {
            Some(x) => Ok(Value::Float(x).ref_cell()),
            None => Ok(Value::Nil.ref_cell()),
        },
        other => Err(type_err(
            span,
            format!(
                "nsimd_min() expects IntArray or FloatArray, got {}",
                other.type_name()
            ),
        )),
    }
}

fn nsimd_max(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsimd_max", span)?;
    match &*args[0].borrow() {
        Value::IntArray(v) => match max_i64_kernel(v) {
            Some(n) => Ok(Value::Int(n).ref_cell()),
            None => Ok(Value::Nil.ref_cell()),
        },
        Value::FloatArray(v) => match max_f64_kernel(v) {
            Some(x) => Ok(Value::Float(x).ref_cell()),
            None => Ok(Value::Nil.ref_cell()),
        },
        other => Err(type_err(
            span,
            format!(
                "nsimd_max() expects IntArray or FloatArray, got {}",
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nsimd_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nsimd_fns![
    ("nsimd_sum", "sum", nsimd_sum),
    ("nsimd_add", "add", nsimd_add),
    ("nsimd_sub", "sub", nsimd_sub),
    ("nsimd_mul", "mul", nsimd_mul),
    ("nsimd_dot", "dot", nsimd_dot),
    ("nsimd_scale", "scale", nsimd_scale),
    ("nsimd_abs", "abs", nsimd_abs),
    ("nsimd_min", "min", nsimd_min),
    ("nsimd_max", "max", nsimd_max),
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

pub const MODULE_NAME: &str = "nsimd";
pub const MODULE_PATHS: &[&str] = &["nsimd", "std/nsimd"];

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

    fn expect_float(r: NiaoResult<ValueRef>) -> f64 {
        match &*r.unwrap().borrow() {
            Value::Float(x) => *x,
            other => panic!("expected float, got {other:?}"),
        }
    }

    fn expect_int_array(r: NiaoResult<ValueRef>) -> Vec<i64> {
        match &*r.unwrap().borrow() {
            Value::IntArray(v) => v.clone(),
            other => panic!("expected int array, got {other:?}"),
        }
    }

    #[test]
    fn sum_unrolled_i64() {
        assert_eq!(sum_i64_kernel(&[]), 0);
        assert_eq!(sum_i64_kernel(&[1, 2, 3, 4, 5, 6, 7, 8, 9]), 45);
        assert_eq!(expect_int(nsimd_sum(&[ia(vec![1, 2, 3])], span())), 6);
    }

    #[test]
    fn add_mul_dot_f64() {
        let a = fa(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
        let b = fa(vec![1.0; 9]);
        assert!((expect_float(nsimd_sum(&[a.clone()], span())) - 45.0).abs() < 1e-12);
        let add = nsimd_add(&[a.clone(), b.clone()], span()).unwrap();
        match &*add.borrow() {
            Value::FloatArray(v) => assert_eq!(v.len(), 9),
            other => panic!("{other:?}"),
        }
        assert!((expect_float(nsimd_dot(&[a, b], span())) - 45.0).abs() < 1e-12);
    }

    #[test]
    fn scale_and_abs() {
        assert_eq!(
            expect_int_array(nsimd_scale(
                &[ia(vec![1, -2, 3]), Value::Int(2).ref_cell()],
                span()
            )),
            vec![2, -4, 6]
        );
        assert_eq!(
            expect_int_array(nsimd_abs(&[ia(vec![-1, 2, -3])], span())),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn length_mismatch_error() {
        let v = nsimd_add(&[ia(vec![1, 2]), ia(vec![1])], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn arity_error() {
        assert!(nsimd_sum(&[], span()).is_err());
    }
}
