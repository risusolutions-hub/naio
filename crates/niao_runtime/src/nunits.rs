//! Native nunits standard library — physical units, quantity arithmetic,
//! conversion, and dimensional checks (~Python `pint`).
//!
//! Import with `import "nunits"` (or `import "std/nunits"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_units::{
    parse_quantity, parse_unit_expr, parse_unit_name, Quantity, Registry, Unit, UnitError,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E4600: u32 = codes::E4600_NUNITS_ARITY;
const E4601: u32 = codes::E4601_NUNITS_ERROR;
const E4602: u32 = codes::E4602_NUNITS_TYPE;
const E4603: u32 = codes::E4603_NUNITS_PARSE;
const E4604: u32 = codes::E4604_NUNITS_DIMENSION;

enum StoreValue {
    Quantity(Quantity),
    Unit(Unit),
}

thread_local! {
    static STORE: RefCell<HashMap<i64, StoreValue>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
}

fn alloc_quantity(q: Quantity) -> i64 {
    alloc(StoreValue::Quantity(q))
}

fn alloc_unit(u: Unit) -> i64 {
    alloc(StoreValue::Unit(u))
}

fn alloc(v: StoreValue) -> i64 {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    STORE.with(|m| m.borrow_mut().insert(id, v));
    id
}

fn with_quantity<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&Quantity) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    STORE.with(|m| match m.borrow().get(&id) {
        Some(StoreValue::Quantity(q)) => Ok(Ok(f(q))),
        Some(_) => Ok(Err(nunits_err(
            span,
            format!("handle {id} is not a quantity"),
        ))),
        None => Ok(Err(nunits_err(
            span,
            format!("invalid quantity handle {id}"),
        ))),
    })
}

fn get_quantity(id: i64, span: Span) -> NiaoResult<Quantity> {
    match with_quantity(id, span, |q| q.clone())? {
        Ok(q) => Ok(q),
        Err(v) => Err(runtime_from_value(span, v)),
    }
}

fn runtime_from_value(span: Span, v: ValueRef) -> RuntimeError {
    let msg = match &*v.borrow() {
        Value::Object(m) => m
            .get("message")
            .map(|x| match &*x.borrow() {
                Value::String(s) => s.clone(),
                _ => "nunits error".into(),
            })
            .unwrap_or_else(|| "nunits error".into()),
        _ => "nunits error".into(),
    };
    RuntimeError::at(span, E4601, msg)
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4602, msg.into())
}

fn nunits_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4601, "nunits_error", msg.into(), span)
}

fn parse_err(span: Span, e: UnitError) -> ValueRef {
    error_value(E4603, "nunits_error", e.to_string(), span)
}

fn dim_err(span: Span, e: UnitError) -> ValueRef {
    error_value(E4604, "nunits_error", e.to_string(), span)
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4600,
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
            E4600,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
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

fn float_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
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

fn optional_int(args: &[ValueRef], idx: usize) -> Option<usize> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Int(n) if *n >= 0 => Some(*n as usize),
        _ => None,
    }
}

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a quantity handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn ok_int(v: i64) -> ValueRef {
    Value::Int(v).ref_cell()
}

fn ok_bool(v: bool) -> ValueRef {
    Value::Bool(v).ref_cell()
}

fn ok_string(s: impl Into<String>) -> ValueRef {
    Value::String(s.into()).ref_cell()
}

fn registry() -> Registry {
    REGISTRY.with(|r| r.borrow().clone())
}

fn with_registry_mut<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    REGISTRY.with(|r| f(&mut r.borrow_mut()))
}

fn resolve_unit(s: &str, span: Span) -> Result<Unit, ValueRef> {
    let reg = registry();
    parse_unit_expr(s, &reg)
        .or_else(|_| parse_unit_name(s, &reg))
        .map_err(|e| parse_err(span, e))
}

fn binary_quantity_op(
    args: &[ValueRef],
    span: Span,
    name: &str,
    op: impl Fn(&Quantity, &Quantity) -> Result<Quantity, UnitError>,
) -> NiaoResult<ValueRef> {
    arity(args, 2, name, span)?;
    let a = get_quantity(handle_arg(args, 0, name, span)?, span)?;
    let b = get_quantity(handle_arg(args, 1, name, span)?, span)?;
    match op(&a, &b) {
        Ok(q) => Ok(ok_int(alloc_quantity(q))),
        Err(e) if matches!(e, UnitError::DimensionMismatch { .. }) => Ok(dim_err(span, e)),
        Err(e) => Ok(nunits_err(span, e.to_string())),
    }
}

// >>> import "nunits"
// >>> nunits.quantity(5, "m")
// 1
fn nunits_quantity(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nunits_quantity", span)?;
    let mag = float_arg(args, 0, "nunits_quantity", span)?;
    let unit_s = string_arg(args, 1, "nunits_quantity", span)?;
    match resolve_unit(&unit_s, span) {
        Err(v) => Ok(v),
        Ok(unit) => Ok(ok_int(alloc_quantity(Quantity::new(mag, unit)))),
    }
}

// >>> nunits.parse("3.5 km")
// 1
fn nunits_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_parse", span)?;
    let s = string_arg(args, 0, "nunits_parse", span)?;
    match parse_quantity(&s, &registry()) {
        Ok((mag, unit)) => Ok(ok_int(alloc_quantity(Quantity::new(mag, unit)))),
        Err(e) => Ok(parse_err(span, e)),
    }
}

// >>> nunits.unit("m/s")
// 1
fn nunits_unit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_unit", span)?;
    let s = string_arg(args, 0, "nunits_unit", span)?;
    match resolve_unit(&s, span) {
        Err(v) => Ok(v),
        Ok(unit) => Ok(ok_int(alloc_unit(unit))),
    }
}

// >>> nunits.valid_unit("meter")
// true
fn nunits_valid_unit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_valid_unit", span)?;
    let s = string_arg(args, 0, "nunits_valid_unit", span)?;
    Ok(ok_bool(resolve_unit(&s, span).is_ok()))
}

// >>> nunits.valid_quantity("10 m")
// true
fn nunits_valid_quantity(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_valid_quantity", span)?;
    let s = string_arg(args, 0, "nunits_valid_quantity", span)?;
    Ok(ok_bool(parse_quantity(&s, &registry()).is_ok()))
}

// >>> let q = nunits.parse("1 km"); nunits.magnitude(q)
// 1000.0
fn nunits_magnitude(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_magnitude", span)?;
    let id = handle_arg(args, 0, "nunits_magnitude", span)?;
    match with_quantity(id, span, |q| q.magnitude())? {
        Ok(m) => Ok(Value::Float(m).ref_cell()),
        Err(v) => Ok(v),
    }
}

// >>> let q = nunits.parse("1 km"); nunits.unit_of(q)
// "km"
fn nunits_unit_of(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_unit_of", span)?;
    let id = handle_arg(args, 0, "nunits_unit_of", span)?;
    match with_quantity(id, span, |q| q.unit.symbol.clone())? {
        Ok(s) => Ok(ok_string(s)),
        Err(v) => Ok(v),
    }
}

// >>> let q = nunits.parse("9.8 m/s^2"); nunits.dimension(q)
// "m/s^2"
fn nunits_dimension(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_dimension", span)?;
    let id = handle_arg(args, 0, "nunits_dimension", span)?;
    match with_quantity(id, span, |q| q.dimension().format())? {
        Ok(s) => Ok(ok_string(s)),
        Err(v) => Ok(v),
    }
}

// >>> let q = nunits.parse("5 m"); nunits.dimensionless(q)
// false
fn nunits_dimensionless(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_dimensionless", span)?;
    let id = handle_arg(args, 0, "nunits_dimensionless", span)?;
    match with_quantity(id, span, |q| q.is_dimensionless())? {
        Ok(b) => Ok(ok_bool(b)),
        Err(v) => Ok(v),
    }
}

// >>> let a = nunits.parse("1 km"); let b = nunits.parse("500 m"); nunits.compatible(a, b)
// true
fn nunits_compatible(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nunits_compatible", span)?;
    let a = get_quantity(handle_arg(args, 0, "nunits_compatible", span)?, span)?;
    let b = get_quantity(handle_arg(args, 1, "nunits_compatible", span)?, span)?;
    Ok(ok_bool(a.compatible(&b)))
}

// >>> let q = nunits.parse("1 km"); nunits.to(q, "m")
// 1
fn nunits_to(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nunits_to", span)?;
    let id = handle_arg(args, 0, "nunits_to", span)?;
    let unit_s = string_arg(args, 1, "nunits_to", span)?;
    let q = get_quantity(id, span)?;
    let target = match resolve_unit(&unit_s, span) {
        Err(v) => return Ok(v),
        Ok(u) => u,
    };
    match q.to_unit(&target) {
        Ok(out) => Ok(ok_int(alloc_quantity(out))),
        Err(e) => Ok(dim_err(span, e)),
    }
}

// >>> let q = nunits.parse("1 km"); nunits.to_base(q)
// 1
fn nunits_to_base(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_to_base", span)?;
    let q = get_quantity(handle_arg(args, 0, "nunits_to_base", span)?, span)?;
    let dim = q.dimension();
    let mut base = Unit::dimensionless();
    base.dimension = dim;
    base.symbol = dim.format();
    if base.symbol == "dimensionless" {
        base.symbol.clear();
    }
    base.scale = 1.0;
    match q.to_unit(&base) {
        Ok(out) => Ok(ok_int(alloc_quantity(out))),
        Err(e) => Ok(dim_err(span, e)),
    }
}

// >>> let q = nunits.parse("1.5 km"); nunits.to_string(q)
// "1.5 km"
fn nunits_to_string(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nunits_to_string", span)?;
    let id = handle_arg(args, 0, "nunits_to_string", span)?;
    let prec = optional_int(args, 1);
    match with_quantity(id, span, |q| q.format(prec))? {
        Ok(s) => Ok(ok_string(s)),
        Err(v) => Ok(v),
    }
}

// >>> let a = nunits.parse("1 m"); let b = nunits.parse("2 m"); nunits.add(a, b)
// 1
fn nunits_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    binary_quantity_op(args, span, "nunits_add", |a, b| a.add(b))
}

// >>> let a = nunits.parse("5 m"); let b = nunits.parse("2 m"); nunits.sub(a, b)
// 1
fn nunits_sub(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    binary_quantity_op(args, span, "nunits_sub", |a, b| a.sub(b))
}

// >>> let a = nunits.parse("3 m"); let b = nunits.parse("4 m"); nunits.mul(a, b)
// 1
fn nunits_mul(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    binary_quantity_op(args, span, "nunits_mul", |a, b| a.mul(b))
}

// >>> let a = nunits.parse("10 m"); let b = nunits.parse("2 s"); nunits.div(a, b)
// 1
fn nunits_div(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    binary_quantity_op(args, span, "nunits_div", |a, b| a.div(b))
}

// >>> let q = nunits.parse("2 m"); nunits.pow(q, 2)
// 1
fn nunits_pow(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nunits_pow", span)?;
    let q = get_quantity(handle_arg(args, 0, "nunits_pow", span)?, span)?;
    let exp = int_arg(args, 1, "nunits_pow", span)?;
    match q.pow(exp as i32) {
        Ok(out) => Ok(ok_int(alloc_quantity(out))),
        Err(e) => Ok(nunits_err(span, e.to_string())),
    }
}

// >>> let q = nunits.parse("5 m"); nunits.neg(q)
// 1
fn nunits_neg(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_neg", span)?;
    let q = get_quantity(handle_arg(args, 0, "nunits_neg", span)?, span)?;
    Ok(ok_int(alloc_quantity(q.neg())))
}

// >>> let q = nunits.parse("-3 m"); nunits.abs(q)
// 1
fn nunits_abs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_abs", span)?;
    let q = get_quantity(handle_arg(args, 0, "nunits_abs", span)?, span)?;
    Ok(ok_int(alloc_quantity(q.abs())))
}

// >>> let q = nunits.parse("4 m^2"); nunits.sqrt(q)
// 1
fn nunits_sqrt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_sqrt", span)?;
    let q = get_quantity(handle_arg(args, 0, "nunits_sqrt", span)?, span)?;
    match q.sqrt() {
        Ok(out) => Ok(ok_int(alloc_quantity(out))),
        Err(e) => Ok(nunits_err(span, e.to_string())),
    }
}

// >>> let a = nunits.parse("2 m"); let b = nunits.parse("5 m"); nunits.compare(a, b)
// -1
fn nunits_compare(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nunits_compare", span)?;
    let a = get_quantity(handle_arg(args, 0, "nunits_compare", span)?, span)?;
    let b = get_quantity(handle_arg(args, 1, "nunits_compare", span)?, span)?;
    match a.compare(&b) {
        Ok(ord) => Ok(Value::Int(ord as i64).ref_cell()),
        Err(e) => Ok(dim_err(span, e)),
    }
}

// >>> let q = nunits.parse("10 m"); nunits.scale(q, 2.5)
// 1
fn nunits_scale(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nunits_scale", span)?;
    let q = get_quantity(handle_arg(args, 0, "nunits_scale", span)?, span)?;
    let factor = float_arg(args, 1, "nunits_scale", span)?;
    Ok(ok_int(alloc_quantity(q.scale(factor))))
}

// >>> let q = nunits.parse("3.14"); nunits.as_float(q)
// 3.14
fn nunits_as_float(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_as_float", span)?;
    let q = get_quantity(handle_arg(args, 0, "nunits_as_float", span)?, span)?;
    if !q.is_dimensionless() {
        return Ok(dim_err(span, UnitError::NotDimensionless));
    }
    Ok(Value::Float(q.magnitude()).ref_cell())
}

// >>> nunits.convert(1, "km", "m")
// 1000.0
fn nunits_convert(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nunits_convert", span)?;
    let mag = float_arg(args, 0, "nunits_convert", span)?;
    let from_s = string_arg(args, 1, "nunits_convert", span)?;
    let to_s = string_arg(args, 2, "nunits_convert", span)?;
    let from = match resolve_unit(&from_s, span) {
        Err(v) => return Ok(v),
        Ok(u) => u,
    };
    let to = match resolve_unit(&to_s, span) {
        Err(v) => return Ok(v),
        Ok(u) => u,
    };
    let q = Quantity::new(mag, from);
    match q.to_unit(&to) {
        Ok(out) => Ok(Value::Float(out.magnitude()).ref_cell()),
        Err(e) => Ok(dim_err(span, e)),
    }
}

// >>> nunits.define("my_km", "1000*m")
// true
fn nunits_define(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nunits_define", span)?;
    let name = string_arg(args, 0, "nunits_define", span)?;
    let expr = string_arg(args, 1, "nunits_define", span)?;
    match with_registry_mut(|reg| reg.define_expr(&name, &expr)) {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(parse_err(span, e)),
    }
}

// >>> len(nunits.definitions()) > 10
// true
fn nunits_definitions(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nunits_definitions", span)?;
    let names = registry().names();
    let arr: Vec<ValueRef> = names.into_iter().map(|s| ok_string(s)).collect();
    Ok(Value::Array(arr).ref_cell())
}

// >>> len(nunits.prefixes()) > 5
// true
fn nunits_prefixes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nunits_prefixes", span)?;
    let arr: Vec<ValueRef> = registry()
        .prefixes()
        .into_iter()
        .map(|s| ok_string(s))
        .collect();
    Ok(Value::Array(arr).ref_cell())
}

// >>> nunits.reset()
// true
fn nunits_reset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nunits_reset", span)?;
    REGISTRY.with(|r| *r.borrow_mut() = Registry::default());
    STORE.with(|m| m.borrow_mut().clear());
    Ok(ok_bool(true))
}

// >>> let q = nunits.parse("1 m"); nunits.close(q)
// true
fn nunits_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunits_close", span)?;
    let id = handle_arg(args, 0, "nunits_close", span)?;
    let removed = STORE.with(|m| m.borrow_mut().remove(&id).is_some());
    Ok(ok_bool(removed))
}

macro_rules! nunits_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nunits_fns![
    ("nunits_quantity", "quantity", nunits_quantity),
    ("nunits_parse", "parse", nunits_parse),
    ("nunits_unit", "unit", nunits_unit),
    ("nunits_valid_unit", "valid_unit", nunits_valid_unit),
    (
        "nunits_valid_quantity",
        "valid_quantity",
        nunits_valid_quantity
    ),
    ("nunits_magnitude", "magnitude", nunits_magnitude),
    ("nunits_unit_of", "unit_of", nunits_unit_of),
    ("nunits_dimension", "dimension", nunits_dimension),
    (
        "nunits_dimensionless",
        "dimensionless",
        nunits_dimensionless
    ),
    ("nunits_compatible", "compatible", nunits_compatible),
    ("nunits_to", "to", nunits_to),
    ("nunits_to_base", "to_base", nunits_to_base),
    ("nunits_to_string", "to_string", nunits_to_string),
    ("nunits_add", "add", nunits_add),
    ("nunits_sub", "sub", nunits_sub),
    ("nunits_mul", "mul", nunits_mul),
    ("nunits_div", "div", nunits_div),
    ("nunits_pow", "pow", nunits_pow),
    ("nunits_neg", "neg", nunits_neg),
    ("nunits_abs", "abs", nunits_abs),
    ("nunits_sqrt", "sqrt", nunits_sqrt),
    ("nunits_compare", "compare", nunits_compare),
    ("nunits_scale", "scale", nunits_scale),
    ("nunits_as_float", "as_float", nunits_as_float),
    ("nunits_convert", "convert", nunits_convert),
    ("nunits_define", "define", nunits_define),
    ("nunits_definitions", "definitions", nunits_definitions),
    ("nunits_prefixes", "prefixes", nunits_prefixes),
    ("nunits_reset", "reset", nunits_reset),
    ("nunits_close", "close", nunits_close),
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
    for (name, sym) in [
        ("METER", "m"),
        ("SECOND", "s"),
        ("KILOGRAM", "kg"),
        ("KELVIN", "K"),
        ("NEWTON", "N"),
        ("PASCAL", "Pa"),
        ("JOULE", "J"),
        ("WATT", "W"),
        ("HERTZ", "Hz"),
    ] {
        map.insert(name.to_string(), ok_string(sym));
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nunits";
pub const MODULE_PATHS: &[&str] = &["nunits", "std/nunits"];

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
    fn parse_km_to_m() {
        let q = nunits_parse(&[Value::String("1 km".into()).ref_cell()], span()).unwrap();
        let m = nunits_to(&[q, Value::String("m".into()).ref_cell()], span()).unwrap();
        let mag = nunits_magnitude(&[m], span()).unwrap();
        let borrowed = mag.borrow();
        match &*borrowed {
            Value::Float(f) => assert!((*f - 1000.0).abs() < 1e-9),
            other => panic!("expected float, got {other:?}"),
        }
    }
}
