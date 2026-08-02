//! Native nmath standard library — scalar math, integer combinatorics, and
//! descriptive statistics. Constants (`nmath.pi`, `nmath.e`, `nmath.tau`,
//! `nmath.inf`, `nmath.nan`) live on the namespace object. Std-only.
//!
//! Import with `import "nmath"` (or `import "std/nmath"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Argument helpers
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
            codes::E2610_NMATH_ARITY,
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
            codes::E2610_NMATH_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

/// Numeric argument: Int, Float, or BigInt → f64.
fn num_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        Value::BigInt(b) => Ok(b.to_string().parse::<f64>().unwrap_or(f64::INFINITY)),
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

fn domain_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2613_NMATH_DOMAIN, "nmath_error", msg.into(), span)
}

fn float_val(f: f64) -> NiaoResult<ValueRef> {
    Ok(Value::Float(f).ref_cell())
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

/// True if argument idx is an Int (used to preserve int-ness).
fn is_int(args: &[ValueRef], idx: usize) -> bool {
    matches!(&*args[idx].borrow(), Value::Int(_))
}

/// Collect numbers from an Array / IntArray / FloatArray value.
fn numbers_from(value: &Value, name: &str, span: Span) -> NiaoResult<Vec<f64>> {
    match value {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::Int(n) => out.push(*n as f64),
                    Value::Float(f) => out.push(*f),
                    Value::BigInt(b) => {
                        out.push(b.to_string().parse::<f64>().unwrap_or(f64::INFINITY))
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!("{name}() expects numbers, found {}", other.type_name()),
                        ))
                    }
                }
            }
            Ok(out)
        }
        Value::IntArray(v) => Ok(v.iter().map(|n| *n as f64).collect()),
        Value::FloatArray(v) => Ok(v.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an array of numbers, got {}",
                other.type_name()
            ),
        )),
    }
}

fn stats_input(args: &[ValueRef], name: &str, span: Span) -> NiaoResult<Vec<f64>> {
    numbers_from(&args[0].borrow(), name, span)
}

// ---------------------------------------------------------------------------
// Unary float functions
// ---------------------------------------------------------------------------

macro_rules! unary_float {
    ($fname:ident, $name:literal, $op:expr) => {
        fn $fname(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
            arity(args, 1, $name, span)?;
            let x = num_arg(args, 0, $name, span)?;
            let f: fn(f64) -> f64 = $op;
            float_val(f(x))
        }
    };
}

unary_float!(nmath_sqrt, "nmath_sqrt", f64::sqrt);
unary_float!(nmath_cbrt, "nmath_cbrt", f64::cbrt);
unary_float!(nmath_exp, "nmath_exp", f64::exp);
unary_float!(nmath_ln, "nmath_ln", f64::ln);
unary_float!(nmath_log2, "nmath_log2", f64::log2);
unary_float!(nmath_log10, "nmath_log10", f64::log10);
unary_float!(nmath_sin, "nmath_sin", f64::sin);
unary_float!(nmath_cos, "nmath_cos", f64::cos);
unary_float!(nmath_tan, "nmath_tan", f64::tan);
unary_float!(nmath_asin, "nmath_asin", f64::asin);
unary_float!(nmath_acos, "nmath_acos", f64::acos);
unary_float!(nmath_atan, "nmath_atan", f64::atan);
unary_float!(nmath_sinh, "nmath_sinh", f64::sinh);
unary_float!(nmath_cosh, "nmath_cosh", f64::cosh);
unary_float!(nmath_tanh, "nmath_tanh", f64::tanh);
unary_float!(nmath_deg, "nmath_deg", f64::to_degrees);
unary_float!(nmath_rad, "nmath_rad", f64::to_radians);

fn nmath_log(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmath_log", span)?;
    let x = num_arg(args, 0, "nmath_log", span)?;
    let base = num_arg(args, 1, "nmath_log", span)?;
    float_val(x.log(base))
}

fn nmath_pow(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmath_pow", span)?;
    let x = num_arg(args, 0, "nmath_pow", span)?;
    let y = num_arg(args, 1, "nmath_pow", span)?;
    // int ^ non-negative int stays int when it fits
    if is_int(args, 0) && is_int(args, 1) {
        let base = x as i64;
        let exp = y as i64;
        if (0..=62).contains(&exp) {
            if let Some(v) = base.checked_pow(exp as u32) {
                return int_val(v);
            }
        }
    }
    float_val(x.powf(y))
}

fn nmath_atan2(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmath_atan2", span)?;
    let y = num_arg(args, 0, "nmath_atan2", span)?;
    let x = num_arg(args, 1, "nmath_atan2", span)?;
    float_val(y.atan2(x))
}

fn nmath_hypot(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmath_hypot", span)?;
    let x = num_arg(args, 0, "nmath_hypot", span)?;
    let y = num_arg(args, 1, "nmath_hypot", span)?;
    float_val(x.hypot(y))
}

// ---------------------------------------------------------------------------
// Rounding & sign
// ---------------------------------------------------------------------------

fn nmath_floor(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_floor", span)?;
    if is_int(args, 0) {
        return int_val(int_arg(args, 0, "nmath_floor", span)?);
    }
    let x = num_arg(args, 0, "nmath_floor", span)?;
    int_val(x.floor() as i64)
}

fn nmath_ceil(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_ceil", span)?;
    if is_int(args, 0) {
        return int_val(int_arg(args, 0, "nmath_ceil", span)?);
    }
    let x = num_arg(args, 0, "nmath_ceil", span)?;
    int_val(x.ceil() as i64)
}

fn nmath_round(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_round", span)?;
    if is_int(args, 0) {
        return int_val(int_arg(args, 0, "nmath_round", span)?);
    }
    let x = num_arg(args, 0, "nmath_round", span)?;
    int_val(x.round() as i64)
}

fn nmath_trunc(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_trunc", span)?;
    if is_int(args, 0) {
        return int_val(int_arg(args, 0, "nmath_trunc", span)?);
    }
    let x = num_arg(args, 0, "nmath_trunc", span)?;
    int_val(x.trunc() as i64)
}

fn nmath_round_to(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmath_round_to", span)?;
    let x = num_arg(args, 0, "nmath_round_to", span)?;
    let decimals = int_arg(args, 1, "nmath_round_to", span)?;
    if !(-15..=15).contains(&decimals) {
        return Err(type_err(
            span,
            "nmath_round_to() decimals must be in -15..=15",
        ));
    }
    let factor = 10f64.powi(decimals as i32);
    float_val((x * factor).round() / factor)
}

fn nmath_abs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_abs", span)?;
    if is_int(args, 0) {
        let n = int_arg(args, 0, "nmath_abs", span)?;
        return int_val(n.saturating_abs());
    }
    let x = num_arg(args, 0, "nmath_abs", span)?;
    float_val(x.abs())
}

fn nmath_sign(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_sign", span)?;
    let x = num_arg(args, 0, "nmath_sign", span)?;
    int_val(if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    })
}

fn nmath_clamp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nmath_clamp", span)?;
    let x = num_arg(args, 0, "nmath_clamp", span)?;
    let lo = num_arg(args, 1, "nmath_clamp", span)?;
    let hi = num_arg(args, 2, "nmath_clamp", span)?;
    if lo > hi {
        return Ok(domain_err(span, "nmath_clamp() requires lo <= hi"));
    }
    let all_int = is_int(args, 0) && is_int(args, 1) && is_int(args, 2);
    let v = x.clamp(lo, hi);
    if all_int {
        int_val(v as i64)
    } else {
        float_val(v)
    }
}

fn nmath_lerp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nmath_lerp", span)?;
    let a = num_arg(args, 0, "nmath_lerp", span)?;
    let b = num_arg(args, 1, "nmath_lerp", span)?;
    let t = num_arg(args, 2, "nmath_lerp", span)?;
    float_val(a + (b - a) * t)
}

fn nmath_map_range(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 5, "nmath_map_range", span)?;
    let x = num_arg(args, 0, "nmath_map_range", span)?;
    let a0 = num_arg(args, 1, "nmath_map_range", span)?;
    let a1 = num_arg(args, 2, "nmath_map_range", span)?;
    let b0 = num_arg(args, 3, "nmath_map_range", span)?;
    let b1 = num_arg(args, 4, "nmath_map_range", span)?;
    if a0 == a1 {
        return Ok(domain_err(span, "nmath_map_range() source range is empty"));
    }
    float_val(b0 + (x - a0) * (b1 - b0) / (a1 - a0))
}

// ---------------------------------------------------------------------------
// Integer combinatorics
// ---------------------------------------------------------------------------

fn gcd_impl(mut a: i64, mut b: i64) -> i64 {
    a = a.saturating_abs();
    b = b.saturating_abs();
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn nmath_gcd(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmath_gcd", span)?;
    let a = int_arg(args, 0, "nmath_gcd", span)?;
    let b = int_arg(args, 1, "nmath_gcd", span)?;
    int_val(gcd_impl(a, b))
}

fn nmath_lcm(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmath_lcm", span)?;
    let a = int_arg(args, 0, "nmath_lcm", span)?;
    let b = int_arg(args, 1, "nmath_lcm", span)?;
    if a == 0 || b == 0 {
        return int_val(0);
    }
    let g = gcd_impl(a, b);
    match (a / g).checked_mul(b) {
        Some(v) => int_val(v.saturating_abs()),
        None => Ok(domain_err(span, "nmath_lcm() overflow")),
    }
}

fn nmath_factorial(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_factorial", span)?;
    let n = int_arg(args, 0, "nmath_factorial", span)?;
    if n < 0 {
        return Ok(domain_err(span, "nmath_factorial() of negative number"));
    }
    if n > 20 {
        return Ok(domain_err(
            span,
            "nmath_factorial() overflows int for n > 20",
        ));
    }
    let mut acc: i64 = 1;
    for k in 2..=n {
        acc *= k;
    }
    int_val(acc)
}

fn comb_impl(n: i64, k: i64) -> Option<i64> {
    if k < 0 || n < 0 || k > n {
        return Some(0);
    }
    let k = k.min(n - k);
    let mut acc: i128 = 1;
    for i in 0..k {
        acc = acc.checked_mul((n - i) as i128)?;
        acc /= (i + 1) as i128;
    }
    i64::try_from(acc).ok()
}

fn nmath_comb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmath_comb", span)?;
    let n = int_arg(args, 0, "nmath_comb", span)?;
    let k = int_arg(args, 1, "nmath_comb", span)?;
    match comb_impl(n, k) {
        Some(v) => int_val(v),
        None => Ok(domain_err(span, "nmath_comb() overflow")),
    }
}

fn nmath_perm(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmath_perm", span)?;
    let n = int_arg(args, 0, "nmath_perm", span)?;
    let k = int_arg(args, 1, "nmath_perm", span)?;
    if k < 0 || n < 0 || k > n {
        return int_val(0);
    }
    let mut acc: i128 = 1;
    for i in 0..k {
        acc = match acc.checked_mul((n - i) as i128) {
            Some(v) => v,
            None => return Ok(domain_err(span, "nmath_perm() overflow")),
        };
    }
    match i64::try_from(acc) {
        Ok(v) => int_val(v),
        Err(_) => Ok(domain_err(span, "nmath_perm() overflow")),
    }
}

// ---------------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------------

fn nmath_is_nan(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_is_nan", span)?;
    let x = num_arg(args, 0, "nmath_is_nan", span)?;
    bool_val(x.is_nan())
}

fn nmath_is_finite(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_is_finite", span)?;
    let x = num_arg(args, 0, "nmath_is_finite", span)?;
    bool_val(x.is_finite())
}

fn nmath_is_inf(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_is_inf", span)?;
    let x = num_arg(args, 0, "nmath_is_inf", span)?;
    bool_val(x.is_infinite())
}

// ---------------------------------------------------------------------------
// min / max — variadic scalars or a single array
// ---------------------------------------------------------------------------

fn min_max_impl(args: &[ValueRef], name: &str, span: Span, want_max: bool) -> NiaoResult<ValueRef> {
    if args.is_empty() {
        return Err(RuntimeError::at(
            span,
            codes::E2610_NMATH_ARITY,
            format!("{name}() expects at least 1 argument"),
        ));
    }
    let is_array_input = args.len() == 1
        && matches!(
            &*args[0].borrow(),
            Value::Array(_) | Value::IntArray(_) | Value::FloatArray(_)
        );
    let (values, all_ints) = if is_array_input {
        let all_ints = matches!(&*args[0].borrow(), Value::IntArray(_))
            || match &*args[0].borrow() {
                Value::Array(items) => items.iter().all(|v| matches!(&*v.borrow(), Value::Int(_))),
                _ => false,
            };
        (numbers_from(&args[0].borrow(), name, span)?, all_ints)
    } else {
        let mut vs = Vec::with_capacity(args.len());
        let mut all_ints = true;
        for idx in 0..args.len() {
            vs.push(num_arg(args, idx, name, span)?);
            all_ints &= is_int(args, idx);
        }
        (vs, all_ints)
    };
    if values.is_empty() {
        return Ok(domain_err(span, format!("{name}() of empty input")));
    }
    let mut best = values[0];
    for &v in &values[1..] {
        if (want_max && v > best) || (!want_max && v < best) {
            best = v;
        }
    }
    if all_ints {
        int_val(best as i64)
    } else {
        float_val(best)
    }
}

fn nmath_min(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    min_max_impl(args, "nmath_min", span, false)
}

fn nmath_max(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    min_max_impl(args, "nmath_max", span, true)
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

fn nmath_sum(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_sum", span)?;
    // Int-preserving fast paths
    match &*args[0].borrow() {
        Value::IntArray(v) => {
            let mut acc: i64 = 0;
            let mut overflow = false;
            for n in v {
                match acc.checked_add(*n) {
                    Some(s) => acc = s,
                    None => {
                        overflow = true;
                        break;
                    }
                }
            }
            if !overflow {
                return int_val(acc);
            }
            return float_val(v.iter().map(|n| *n as f64).sum());
        }
        Value::FloatArray(v) => return float_val(v.iter().sum()),
        _ => {}
    }
    let values = stats_input(args, "nmath_sum", span)?;
    float_val(values.iter().sum())
}

fn nmath_mean(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_mean", span)?;
    let values = stats_input(args, "nmath_mean", span)?;
    if values.is_empty() {
        return Ok(domain_err(span, "nmath_mean() of empty input"));
    }
    float_val(values.iter().sum::<f64>() / values.len() as f64)
}

fn nmath_median(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_median", span)?;
    let mut values = stats_input(args, "nmath_median", span)?;
    if values.is_empty() {
        return Ok(domain_err(span, "nmath_median() of empty input"));
    }
    values.sort_by(f64::total_cmp);
    let n = values.len();
    let mid = n / 2;
    if n % 2 == 1 {
        float_val(values[mid])
    } else {
        float_val((values[mid - 1] + values[mid]) / 2.0)
    }
}

fn nmath_mode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmath_mode", span)?;
    let values = stats_input(args, "nmath_mode", span)?;
    if values.is_empty() {
        return Ok(domain_err(span, "nmath_mode() of empty input"));
    }
    let mut counts: HashMap<u64, (usize, f64)> = HashMap::with_capacity(values.len());
    for &v in &values {
        let entry = counts.entry(v.to_bits()).or_insert((0, v));
        entry.0 += 1;
    }
    let mut best = (0usize, f64::INFINITY);
    for (_, (count, v)) in counts {
        if count > best.0 || (count == best.0 && v < best.1) {
            best = (count, v);
        }
    }
    float_val(best.1)
}

/// Sample variance by default; pass `true` as second arg for population variance.
fn variance_impl(args: &[ValueRef], name: &str, span: Span) -> NiaoResult<Result<f64, ValueRef>> {
    let values = stats_input(args, name, span)?;
    let population = args
        .get(1)
        .map(|v| matches!(&*v.borrow(), Value::Bool(true)))
        .unwrap_or(false);
    let n = values.len();
    if (population && n == 0) || (!population && n < 2) {
        return Ok(Err(domain_err(
            span,
            format!(
                "{name}() needs at least {} value(s)",
                if population { 1 } else { 2 }
            ),
        )));
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    let ss: f64 = values.iter().map(|v| (v - mean) * (v - mean)).sum();
    let denom = if population { n } else { n - 1 } as f64;
    Ok(Ok(ss / denom))
}

fn nmath_variance(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmath_variance", span)?;
    match variance_impl(args, "nmath_variance", span)? {
        Ok(v) => float_val(v),
        Err(e) => Ok(e),
    }
}

fn nmath_stdev(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmath_stdev", span)?;
    match variance_impl(args, "nmath_stdev", span)? {
        Ok(v) => float_val(v.sqrt()),
        Err(e) => Ok(e),
    }
}

fn nmath_percentile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmath_percentile", span)?;
    let mut values = stats_input(args, "nmath_percentile", span)?;
    let p = num_arg(args, 1, "nmath_percentile", span)?;
    if values.is_empty() {
        return Ok(domain_err(span, "nmath_percentile() of empty input"));
    }
    if !(0.0..=100.0).contains(&p) {
        return Ok(domain_err(span, "nmath_percentile() p must be in 0..=100"));
    }
    values.sort_by(f64::total_cmp);
    let rank = p / 100.0 * (values.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        return float_val(values[lo]);
    }
    let frac = rank - lo as f64;
    float_val(values[lo] + (values[hi] - values[lo]) * frac)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nmath_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nmath_fns![
    ("nmath_sqrt", "sqrt", nmath_sqrt),
    ("nmath_cbrt", "cbrt", nmath_cbrt),
    ("nmath_pow", "pow", nmath_pow),
    ("nmath_exp", "exp", nmath_exp),
    ("nmath_ln", "ln", nmath_ln),
    ("nmath_log", "log", nmath_log),
    ("nmath_log2", "log2", nmath_log2),
    ("nmath_log10", "log10", nmath_log10),
    ("nmath_sin", "sin", nmath_sin),
    ("nmath_cos", "cos", nmath_cos),
    ("nmath_tan", "tan", nmath_tan),
    ("nmath_asin", "asin", nmath_asin),
    ("nmath_acos", "acos", nmath_acos),
    ("nmath_atan", "atan", nmath_atan),
    ("nmath_atan2", "atan2", nmath_atan2),
    ("nmath_sinh", "sinh", nmath_sinh),
    ("nmath_cosh", "cosh", nmath_cosh),
    ("nmath_tanh", "tanh", nmath_tanh),
    ("nmath_hypot", "hypot", nmath_hypot),
    ("nmath_floor", "floor", nmath_floor),
    ("nmath_ceil", "ceil", nmath_ceil),
    ("nmath_round", "round", nmath_round),
    ("nmath_trunc", "trunc", nmath_trunc),
    ("nmath_round_to", "round_to", nmath_round_to),
    ("nmath_abs", "abs", nmath_abs),
    ("nmath_sign", "sign", nmath_sign),
    ("nmath_clamp", "clamp", nmath_clamp),
    ("nmath_lerp", "lerp", nmath_lerp),
    ("nmath_map_range", "map_range", nmath_map_range),
    ("nmath_gcd", "gcd", nmath_gcd),
    ("nmath_lcm", "lcm", nmath_lcm),
    ("nmath_factorial", "factorial", nmath_factorial),
    ("nmath_comb", "comb", nmath_comb),
    ("nmath_perm", "perm", nmath_perm),
    ("nmath_deg", "deg", nmath_deg),
    ("nmath_rad", "rad", nmath_rad),
    ("nmath_is_nan", "is_nan", nmath_is_nan),
    ("nmath_is_finite", "is_finite", nmath_is_finite),
    ("nmath_is_inf", "is_inf", nmath_is_inf),
    ("nmath_min", "min", nmath_min),
    ("nmath_max", "max", nmath_max),
    ("nmath_sum", "sum", nmath_sum),
    ("nmath_mean", "mean", nmath_mean),
    ("nmath_median", "median", nmath_median),
    ("nmath_mode", "mode", nmath_mode),
    ("nmath_variance", "variance", nmath_variance),
    ("nmath_stdev", "stdev", nmath_stdev),
    ("nmath_percentile", "percentile", nmath_percentile),
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
    // Constants
    map.insert(
        "pi".to_string(),
        Value::Float(std::f64::consts::PI).ref_cell(),
    );
    map.insert(
        "e".to_string(),
        Value::Float(std::f64::consts::E).ref_cell(),
    );
    map.insert(
        "tau".to_string(),
        Value::Float(std::f64::consts::TAU).ref_cell(),
    );
    map.insert("inf".to_string(), Value::Float(f64::INFINITY).ref_cell());
    map.insert("nan".to_string(), Value::Float(f64::NAN).ref_cell());
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nmath";
pub const MODULE_PATHS: &[&str] = &["nmath", "std/nmath"];

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

    fn f(v: f64) -> ValueRef {
        Value::Float(v).ref_cell()
    }

    fn expect_f(r: NiaoResult<ValueRef>) -> f64 {
        match &*r.unwrap().borrow() {
            Value::Float(v) => *v,
            Value::Int(v) => *v as f64,
            other => panic!("expected number, got {other:?}"),
        }
    }

    fn expect_i(r: NiaoResult<ValueRef>) -> i64 {
        match &*r.unwrap().borrow() {
            Value::Int(v) => *v,
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[test]
    fn basic_scalars() {
        assert!((expect_f(nmath_sqrt(&[f(9.0)], span())) - 3.0).abs() < 1e-12);
        assert_eq!(expect_i(nmath_pow(&[i(2), i(10)], span())), 1024);
        assert_eq!(expect_i(nmath_round(&[f(2.5)], span())), 3);
        assert_eq!(expect_i(nmath_sign(&[f(-4.2)], span())), -1);
    }

    #[test]
    fn combinatorics() {
        assert_eq!(expect_i(nmath_gcd(&[i(12), i(18)], span())), 6);
        assert_eq!(expect_i(nmath_lcm(&[i(4), i(6)], span())), 12);
        assert_eq!(expect_i(nmath_factorial(&[i(10)], span())), 3_628_800);
        assert_eq!(expect_i(nmath_comb(&[i(10), i(3)], span())), 120);
        assert_eq!(expect_i(nmath_perm(&[i(5), i(2)], span())), 20);
    }

    #[test]
    fn stats() {
        let arr = Value::IntArray(vec![1, 2, 3, 4, 5]).ref_cell();
        assert_eq!(expect_i(nmath_sum(&[arr.clone()], span())), 15);
        assert!((expect_f(nmath_mean(&[arr.clone()], span())) - 3.0).abs() < 1e-12);
        assert!((expect_f(nmath_median(&[arr.clone()], span())) - 3.0).abs() < 1e-12);
        let sd = expect_f(nmath_stdev(&[arr.clone()], span()));
        assert!((sd - 1.5811388300841898).abs() < 1e-9);
        let p50 = expect_f(nmath_percentile(&[arr, f(50.0)], span()));
        assert!((p50 - 3.0).abs() < 1e-12);
    }

    #[test]
    fn min_max_mixed() {
        assert_eq!(expect_i(nmath_max(&[i(3), i(9), i(4)], span())), 9);
        let arr = Value::FloatArray(vec![2.5, -1.0, 7.25]).ref_cell();
        assert!((expect_f(nmath_min(&[arr], span())) + 1.0).abs() < 1e-12);
    }

    #[test]
    fn domain_errors_are_values() {
        let r = nmath_factorial(&[i(-1)], span()).unwrap();
        assert!(matches!(&*r.borrow(), Value::Error(_)));
        let empty = Value::Array(vec![]).ref_cell();
        let r = nmath_mean(&[empty], span()).unwrap();
        assert!(matches!(&*r.borrow(), Value::Error(_)));
    }
}
