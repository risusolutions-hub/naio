//! Native ndecimal standard library — arbitrary-precision decimals and exact
//! rationals with money-safe rounding modes (~Python `decimal` + `fractions`).
//!
//! Import with `import "ndecimal"` (or `import "std/ndecimal"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_bignum::BigInt;
use niao_decimal::{parse_decimal, parse_fraction, Context, Decimal, Fraction, RoundingMode};
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Handle store
// ---------------------------------------------------------------------------

enum NDecValue {
    Decimal(Decimal),
    Fraction(Fraction),
}

thread_local! {
    static VALUES: RefCell<HashMap<i64, NDecValue>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
    static CTX: RefCell<Context> = RefCell::new(Context::default());
}

fn alloc_decimal(d: Decimal) -> i64 {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    VALUES.with(|m| m.borrow_mut().insert(id, NDecValue::Decimal(d)));
    id
}

fn alloc_fraction(f: Fraction) -> i64 {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    VALUES.with(|m| m.borrow_mut().insert(id, NDecValue::Fraction(f)));
    id
}

fn with_decimal<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&Decimal) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    VALUES.with(|m| {
        match m.borrow().get(&id) {
            Some(NDecValue::Decimal(d)) => Ok(Ok(f(d))),
            Some(_) => Ok(Err(ndecimal_err(
                span,
                format!("handle {id} is not a decimal"),
            ))),
            None => Ok(Err(ndecimal_err(
                span,
                format!("invalid decimal handle {id}"),
            ))),
        }
    })
}

fn with_fraction<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&Fraction) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    VALUES.with(|m| {
        match m.borrow().get(&id) {
            Some(NDecValue::Fraction(fr)) => Ok(Ok(f(fr))),
            Some(_) => Ok(Err(ndecimal_err(
                span,
                format!("handle {id} is not a fraction"),
            ))),
            None => Ok(Err(ndecimal_err(
                span,
                format!("invalid fraction handle {id}"),
            ))),
        }
    })
}

fn with_decimal_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Decimal) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    VALUES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(NDecValue::Decimal(d)) => Ok(Ok(f(d))),
            Some(_) => Ok(Err(ndecimal_err(
                span,
                format!("handle {id} is not a decimal"),
            ))),
            None => Ok(Err(ndecimal_err(
                span,
                format!("invalid decimal handle {id}"),
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4102_NDECIMAL_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E4100_NDECIMAL_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4100_NDECIMAL_ARITY,
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

fn optional_string(args: &[ValueRef], idx: usize) -> Option<String> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn optional_int(args: &[ValueRef], idx: usize, default: i64) -> i64 {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Int(n) => *n,
        _ => default,
    }
}

fn ndecimal_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4101_NDECIMAL_ERROR, "ndecimal_error", msg.into(), span)
}

fn parse_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4103_NDECIMAL_PARSE, "ndecimal_error", msg.into(), span)
}

fn get_decimal(id: i64, span: Span) -> NiaoResult<Decimal> {
    VALUES.with(|m| match m.borrow().get(&id) {
        Some(NDecValue::Decimal(d)) => Ok(d.clone()),
        Some(_) => Err(type_err(span, format!("handle {id} is not a decimal"))),
        None => Err(type_err(span, format!("invalid decimal handle {id}"))),
    })
}

fn get_fraction(id: i64, span: Span) -> NiaoResult<Fraction> {
    VALUES.with(|m| match m.borrow().get(&id) {
        Some(NDecValue::Fraction(f)) => Ok(f.clone()),
        Some(_) => Err(type_err(span, format!("handle {id} is not a fraction"))),
        None => Err(type_err(span, format!("invalid fraction handle {id}"))),
    })
}

fn dec_handle(v: &ValueRef, name: &str, span: Span) -> NiaoResult<i64> {
    match &*v.borrow() {
        Value::Int(id) if *id > 0 => Ok(*id),
        Value::String(s) => {
            let d = parse_decimal(s).map_err(|e| type_err(span, e.to_string()))?;
            Ok(alloc_decimal(d))
        }
        Value::Int(n) => Ok(alloc_decimal(Decimal::from_i64(*n))),
        Value::Float(f) => {
            let d = Decimal::from_f64_repr(*f).map_err(|e| type_err(span, e.to_string()))?;
            Ok(alloc_decimal(d))
        }
        Value::BigInt(b) => {
            let s = b.to_string();
            let bi = BigInt::from_str(&s).unwrap_or_else(|_| BigInt::zero());
            let sign = if bi < BigInt::from(0) {
                niao_bignum::Sign::Minus
            } else {
                niao_bignum::Sign::Plus
            };
            Ok(alloc_decimal(Decimal::from_coeff_exp(
                sign,
                bi.abs(),
                0,
            )))
        }
        other => Err(type_err(
            span,
            format!("{name}() expects decimal handle/string/number, got {}", other.type_name()),
        )),
    }
}

fn frac_handle(v: &ValueRef, name: &str, span: Span) -> NiaoResult<i64> {
    match &*v.borrow() {
        Value::Int(id) if *id > 0 => Ok(*id),
        Value::String(s) => {
            let f = parse_fraction(s).map_err(|e| type_err(span, e.to_string()))?;
            Ok(alloc_fraction(f))
        }
        Value::Int(n) => Ok(alloc_fraction(Fraction::from_raw(
            BigInt::from(*n),
            BigInt::from(1),
        ))),
        Value::BigInt(b) => Ok(alloc_fraction(Fraction::from_raw(b.clone(), BigInt::from(1)))),
        other => Err(type_err(
            span,
            format!("{name}() expects fraction handle/string/int, got {}", other.type_name()),
        )),
    }
}

fn ctx_local() -> Context {
    CTX.with(|c| c.borrow().clone())
}

fn rounding_arg(args: &[ValueRef], idx: usize) -> RoundingMode {
    optional_string(args, idx)
        .and_then(|s| RoundingMode::from_name(&s))
        .unwrap_or_else(|| ctx_local().rounding)
}

fn dec_result(span: Span, r: Result<Decimal, niao_decimal::DecimalError>) -> ValueRef {
    match r {
        Ok(d) => Value::Int(alloc_decimal(d)).ref_cell(),
        Err(e) => ndecimal_err(span, e.to_string()),
    }
}

fn frac_result(span: Span, r: Result<Fraction, niao_decimal::DecimalError>) -> ValueRef {
    match r {
        Ok(f) => Value::Int(alloc_fraction(f)).ref_cell(),
        Err(e) => ndecimal_err(span, e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> import "ndecimal"
// >>> ndecimal.decimal("1.23")
// => 1
fn ndecimal_decimal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_decimal", span)?;
    match &*args[0].borrow() {
        Value::String(s) => match parse_decimal(s) {
            Ok(d) => Ok(Value::Int(alloc_decimal(d)).ref_cell()),
            Err(e) => Ok(parse_err(span, e.to_string())),
        },
        Value::Int(n) => Ok(Value::Int(alloc_decimal(Decimal::from_i64(*n))).ref_cell()),
        Value::Float(f) => match Decimal::from_f64_repr(*f) {
            Ok(d) => Ok(Value::Int(alloc_decimal(d)).ref_cell()),
            Err(e) => Ok(parse_err(span, e.to_string())),
        },
        other => Err(type_err(
            span,
            format!("ndecimal.decimal() expects string or number, got {}", other.type_name()),
        )),
    }
}

// >>> ndecimal.fraction(1, 3)
// => 2
fn ndecimal_fraction(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndecimal_fraction", span)?;
    let numer = match &*args[0].borrow() {
        Value::Int(n) => BigInt::from(*n),
        Value::BigInt(b) => b.clone(),
        Value::String(s) => BigInt::from_str(s).map_err(|_| type_err(span, "invalid numerator"))?,
        other => {
            return Err(type_err(
                span,
                format!("ndecimal.fraction() numerator must be int, got {}", other.type_name()),
            ))
        }
    };
    let denom = if args.len() == 2 {
        match &*args[1].borrow() {
            Value::Int(n) => BigInt::from(*n),
            Value::BigInt(b) => b.clone(),
            Value::String(s) => {
                BigInt::from_str(s).map_err(|_| type_err(span, "invalid denominator"))?
            }
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "ndecimal.fraction() denominator must be int, got {}",
                        other.type_name()
                    ),
                ))
            }
        }
    } else {
        BigInt::from(1)
    };
    match Fraction::new(numer, denom) {
        Ok(f) => Ok(Value::Int(alloc_fraction(f)).ref_cell()),
        Err(e) => Ok(parse_err(span, e.to_string())),
    }
}

fn ndecimal_valid_decimal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_valid_decimal", span)?;
    let s = string_arg(args, 0, "ndecimal_valid_decimal", span)?;
    Ok(Value::Bool(parse_decimal(&s).is_ok()).ref_cell())
}

fn ndecimal_valid_fraction(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_valid_fraction", span)?;
    let s = string_arg(args, 0, "ndecimal_valid_fraction", span)?;
    Ok(Value::Bool(parse_fraction(&s).is_ok()).ref_cell())
}

fn ndecimal_context(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 2, "ndecimal_context", span)?;
    let mut ctx = Context::default();
    if args.len() >= 1 {
        ctx.prec = int_arg(args, 0, "ndecimal_context", span)? as u32;
    }
    if args.len() == 2 {
        let mode = string_arg(args, 1, "ndecimal_context", span)?;
        ctx.rounding = RoundingMode::from_name(&mode).ok_or_else(|| {
            type_err(span, format!("unknown rounding mode '{mode}'"))
        })?;
    }
    CTX.with(|c| *c.borrow_mut() = ctx.clone());
    let mut map = HashMap::new();
    map.insert("prec".to_string(), Value::Int(ctx.prec as i64).ref_cell());
    map.insert(
        "rounding".to_string(),
        Value::String(ctx.rounding.as_name().into()).ref_cell(),
    );
    Ok(Value::Object(map).ref_cell())
}

fn ndecimal_get_context(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ndecimal_get_context", span)?;
    let ctx = ctx_local();
    let mut map = HashMap::new();
    map.insert("prec".to_string(), Value::Int(ctx.prec as i64).ref_cell());
    map.insert(
        "rounding".to_string(),
        Value::String(ctx.rounding.as_name().into()).ref_cell(),
    );
    Ok(Value::Object(map).ref_cell())
}

fn ndecimal_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndecimal_add", span)?;
    let a = get_decimal(dec_handle(&args[0], "ndecimal_add", span)?, span)?;
    let b = get_decimal(dec_handle(&args[1], "ndecimal_add", span)?, span)?;
    Ok(dec_result(span, a.add(&b, &ctx_local())))
}

fn ndecimal_sub(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndecimal_sub", span)?;
    let a = get_decimal(dec_handle(&args[0], "ndecimal_sub", span)?, span)?;
    let b = get_decimal(dec_handle(&args[1], "ndecimal_sub", span)?, span)?;
    Ok(dec_result(span, a.sub(&b, &ctx_local())))
}

fn ndecimal_mul(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndecimal_mul", span)?;
    let a = get_decimal(dec_handle(&args[0], "ndecimal_mul", span)?, span)?;
    let b = get_decimal(dec_handle(&args[1], "ndecimal_mul", span)?, span)?;
    Ok(dec_result(span, a.mul(&b, &ctx_local())))
}

fn ndecimal_div(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndecimal_div", span)?;
    let a = get_decimal(dec_handle(&args[0], "ndecimal_div", span)?, span)?;
    let b = get_decimal(dec_handle(&args[1], "ndecimal_div", span)?, span)?;
    Ok(dec_result(span, a.div(&b, &ctx_local())))
}

fn ndecimal_mod(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndecimal_mod", span)?;
    let a = get_decimal(dec_handle(&args[0], "ndecimal_mod", span)?, span)?;
    let b = get_decimal(dec_handle(&args[1], "ndecimal_mod", span)?, span)?;
    Ok(dec_result(span, a.rem(&b, &ctx_local())))
}

fn ndecimal_pow(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndecimal_pow", span)?;
    let a = get_decimal(dec_handle(&args[0], "ndecimal_pow", span)?, span)?;
    let exp = int_arg(args, 1, "ndecimal_pow", span)?;
    Ok(dec_result(span, a.pow(exp, &ctx_local())))
}

fn ndecimal_abs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_abs", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_abs", span)?, span)?;
    Ok(Value::Int(alloc_decimal(d.abs())).ref_cell())
}

fn ndecimal_neg(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_neg", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_neg", span)?, span)?;
    Ok(Value::Int(alloc_decimal(d.neg())).ref_cell())
}

fn ndecimal_compare(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndecimal_compare", span)?;
    let a = get_decimal(dec_handle(&args[0], "ndecimal_compare", span)?, span)?;
    let b = get_decimal(dec_handle(&args[1], "ndecimal_compare", span)?, span)?;
    match a.compare(&b) {
        Some(ord) => Ok(Value::Int(ord as i64).ref_cell()),
        None => Ok(ndecimal_err(span, "comparison with NaN")),
    }
}

fn ndecimal_quantize(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ndecimal_quantize", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_quantize", span)?, span)?;
    let exp = int_arg(args, 1, "ndecimal_quantize", span)?;
    let mut ctx = ctx_local();
    if args.len() == 3 {
        if let Some(mode) = optional_string(args, 2) {
            ctx.rounding = RoundingMode::from_name(&mode).ok_or_else(|| {
                type_err(span, format!("unknown rounding mode '{mode}'"))
            })?;
        }
    }
    Ok(dec_result(span, d.quantize(exp, &ctx)))
}

fn ndecimal_normalize(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_normalize", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_normalize", span)?, span)?;
    Ok(Value::Int(alloc_decimal(d.normalize())).ref_cell())
}

fn ndecimal_to_integral(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndecimal_to_integral", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_to_integral", span)?, span)?;
    let mut ctx = ctx_local();
    if args.len() == 2 {
        if let Some(mode) = optional_string(args, 1) {
            ctx.rounding = RoundingMode::from_name(&mode).ok_or_else(|| {
                type_err(span, format!("unknown rounding mode '{mode}'"))
            })?;
        }
    }
    Ok(dec_result(span, d.to_integral(&ctx)))
}

fn ndecimal_to_string(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_to_string", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_to_string", span)?, span)?;
    Ok(Value::String(d.to_string()).ref_cell())
}

fn ndecimal_to_sci(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_to_sci", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_to_sci", span)?, span)?;
    Ok(Value::String(d.to_sci_string()).ref_cell())
}

fn ndecimal_to_eng(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_to_eng", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_to_eng", span)?, span)?;
    Ok(Value::String(d.to_eng_string()).ref_cell())
}

fn ndecimal_as_tuple(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_as_tuple", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_as_tuple", span)?, span)?;
    let mut map = HashMap::new();
    if let Some((sign, coeff, exp)) = d.as_tuple() {
        let sign_i = match sign {
            niao_bignum::Sign::Minus => -1,
            _ => 1,
        };
        map.insert("sign".to_string(), Value::Int(sign_i).ref_cell());
        map.insert("coeff".to_string(), Value::String(coeff.to_string()).ref_cell());
        map.insert("exp".to_string(), Value::Int(exp).ref_cell());
    }
    Ok(Value::Object(map).ref_cell())
}

fn ndecimal_is_zero(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_is_zero", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_is_zero", span)?, span)?;
    Ok(Value::Bool(d.is_zero()).ref_cell())
}

fn ndecimal_is_finite(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_is_finite", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_is_finite", span)?, span)?;
    Ok(Value::Bool(d.is_finite()).ref_cell())
}

fn ndecimal_is_nan(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_is_nan", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_is_nan", span)?, span)?;
    Ok(Value::Bool(d.is_nan()).ref_cell())
}

fn ndecimal_is_inf(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_is_inf", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_is_inf", span)?, span)?;
    Ok(Value::Bool(d.is_infinite()).ref_cell())
}

fn ndecimal_sqrt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_sqrt", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_sqrt", span)?, span)?;
    Ok(dec_result(span, d.sqrt(&ctx_local())))
}

fn ndecimal_from_float(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_from_float", span)?;
    match &*args[0].borrow() {
        Value::Float(f) => match Decimal::from_f64_repr(*f) {
            Ok(d) => Ok(Value::Int(alloc_decimal(d)).ref_cell()),
            Err(e) => Ok(parse_err(span, e.to_string())),
        },
        other => Err(type_err(
            span,
            format!("ndecimal.from_float() expects float, got {}", other.type_name()),
        )),
    }
}

// >>> ndecimal.round_money(ndecimal.decimal("2.675"))
// => 3
fn ndecimal_round_money(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "ndecimal_round_money", span)?;
    let d = get_decimal(dec_handle(&args[0], "ndecimal_round_money", span)?, span)?;
    let places = optional_int(args, 1, 2);
    let mut ctx = Context::money();
    if args.len() == 3 {
        if let Some(mode) = optional_string(args, 2) {
            ctx.rounding = RoundingMode::from_name(&mode).ok_or_else(|| {
                type_err(span, format!("unknown rounding mode '{mode}'"))
            })?;
        }
    }
    Ok(dec_result(span, d.quantize(-(places as i64), &ctx)))
}

fn ndecimal_numer(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_numer", span)?;
    let f = get_fraction(frac_handle(&args[0], "ndecimal_numer", span)?, span)?;
    Ok(Value::String(f.numer().to_string()).ref_cell())
}

fn ndecimal_denom(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_denom", span)?;
    let f = get_fraction(frac_handle(&args[0], "ndecimal_denom", span)?, span)?;
    Ok(Value::String(f.denom().to_string()).ref_cell())
}

fn ndecimal_frac_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndecimal_frac_add", span)?;
    let a = get_fraction(frac_handle(&args[0], "ndecimal_frac_add", span)?, span)?;
    let b = get_fraction(frac_handle(&args[1], "ndecimal_frac_add", span)?, span)?;
    Ok(frac_result(span, a.add(&b)))
}

fn ndecimal_frac_mul(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndecimal_frac_mul", span)?;
    let a = get_fraction(frac_handle(&args[0], "ndecimal_frac_mul", span)?, span)?;
    let b = get_fraction(frac_handle(&args[1], "ndecimal_frac_mul", span)?, span)?;
    Ok(frac_result(span, a.mul(&b)))
}

fn ndecimal_frac_div(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndecimal_frac_div", span)?;
    let a = get_fraction(frac_handle(&args[0], "ndecimal_frac_div", span)?, span)?;
    let b = get_fraction(frac_handle(&args[1], "ndecimal_frac_div", span)?, span)?;
    Ok(frac_result(span, a.div(&b)))
}

fn ndecimal_limit_denominator(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndecimal_limit_denominator", span)?;
    let f = get_fraction(frac_handle(&args[0], "ndecimal_limit_denominator", span)?, span)?;
    let max_d = if args.len() == 2 {
        match &*args[1].borrow() {
            Value::Int(n) => BigInt::from(*n),
            Value::BigInt(b) => b.clone(),
            other => {
                return Err(type_err(
                    span,
                    format!("limit_denominator max must be int, got {}", other.type_name()),
                ))
            }
        }
    } else {
        BigInt::from(10_000)
    };
    Ok(Value::Int(alloc_fraction(f.limit_denominator(&max_d))).ref_cell())
}

fn ndecimal_to_decimal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndecimal_to_decimal", span)?;
    let f = get_fraction(frac_handle(&args[0], "ndecimal_to_decimal", span)?, span)?;
    Ok(dec_result(span, Decimal::from_fraction(&f, &ctx_local())))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ndecimal_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ndecimal_fns![
    ("ndecimal_decimal", "decimal", ndecimal_decimal),
    ("ndecimal_fraction", "fraction", ndecimal_fraction),
    ("ndecimal_valid_decimal", "valid_decimal", ndecimal_valid_decimal),
    ("ndecimal_valid_fraction", "valid_fraction", ndecimal_valid_fraction),
    ("ndecimal_context", "context", ndecimal_context),
    ("ndecimal_get_context", "get_context", ndecimal_get_context),
    ("ndecimal_add", "add", ndecimal_add),
    ("ndecimal_sub", "sub", ndecimal_sub),
    ("ndecimal_mul", "mul", ndecimal_mul),
    ("ndecimal_div", "div", ndecimal_div),
    ("ndecimal_mod", "mod", ndecimal_mod),
    ("ndecimal_pow", "pow", ndecimal_pow),
    ("ndecimal_abs", "abs", ndecimal_abs),
    ("ndecimal_neg", "neg", ndecimal_neg),
    ("ndecimal_compare", "compare", ndecimal_compare),
    ("ndecimal_quantize", "quantize", ndecimal_quantize),
    ("ndecimal_normalize", "normalize", ndecimal_normalize),
    ("ndecimal_to_integral", "to_integral", ndecimal_to_integral),
    ("ndecimal_to_string", "to_string", ndecimal_to_string),
    ("ndecimal_to_sci", "to_sci", ndecimal_to_sci),
    ("ndecimal_to_eng", "to_eng", ndecimal_to_eng),
    ("ndecimal_as_tuple", "as_tuple", ndecimal_as_tuple),
    ("ndecimal_is_zero", "is_zero", ndecimal_is_zero),
    ("ndecimal_is_finite", "is_finite", ndecimal_is_finite),
    ("ndecimal_is_nan", "is_nan", ndecimal_is_nan),
    ("ndecimal_is_inf", "is_inf", ndecimal_is_inf),
    ("ndecimal_sqrt", "sqrt", ndecimal_sqrt),
    ("ndecimal_from_float", "from_float", ndecimal_from_float),
    ("ndecimal_round_money", "round_money", ndecimal_round_money),
    ("ndecimal_numer", "numer", ndecimal_numer),
    ("ndecimal_denom", "denom", ndecimal_denom),
    ("ndecimal_frac_add", "frac_add", ndecimal_frac_add),
    ("ndecimal_frac_mul", "frac_mul", ndecimal_frac_mul),
    ("ndecimal_frac_div", "frac_div", ndecimal_frac_div),
    ("ndecimal_limit_denominator", "limit_denominator", ndecimal_limit_denominator),
    ("ndecimal_to_decimal", "to_decimal", ndecimal_to_decimal),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    for (name, mode) in [
        ("ROUND_CEILING", RoundingMode::Ceiling),
        ("ROUND_FLOOR", RoundingMode::Floor),
        ("ROUND_DOWN", RoundingMode::Down),
        ("ROUND_UP", RoundingMode::Up),
        ("ROUND_HALF_UP", RoundingMode::HalfUp),
        ("ROUND_HALF_EVEN", RoundingMode::HalfEven),
        ("ROUND_HALF_DOWN", RoundingMode::HalfDown),
        ("ROUND_05UP", RoundingMode::ZeroFiveUp),
    ] {
        map.insert(
            name.to_string(),
            Value::String(mode.as_name().into()).ref_cell(),
        );
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "ndecimal";
pub const MODULE_PATHS: &[&str] = &["ndecimal", "std/ndecimal"];

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
    fn decimal_add_money() {
        let a = ndecimal_decimal(&[Value::String("1.10".into()).ref_cell()], span()).unwrap();
        let b = ndecimal_decimal(&[Value::String("2.30".into()).ref_cell()], span()).unwrap();
        let c = ndecimal_add(&[a, b], span()).unwrap();
        let s = ndecimal_to_string(&[c], span()).unwrap();
        assert_eq!(s.borrow().to_string(), "3.40");
    }
}
