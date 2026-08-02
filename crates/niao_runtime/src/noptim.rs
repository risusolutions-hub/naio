//! Native noptim — scalar root finding and finite-difference gradients.

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_optim::{root_scalar, RootScalarMethod, ScalarMethod};
use std::collections::HashMap;
use std::rc::Rc;

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E4030_NOPTIM_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn num_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(RuntimeError::at(
            span,
            codes::E4032_NOPTIM_TYPE,
            format!("{name}() expects number, got {}", other.type_name()),
        )),
    }
}

/// Brent root of x^2 - 2 on [0, 2] — demo builtin; general closure API deferred.
fn noptim_sqrt2_root(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "noptim_sqrt2_root", span)?;
    let f = |x: f64| x * x - 2.0;
    let r = root_scalar(
        f,
        (0.0, 2.0),
        RootScalarMethod::Brent,
        None::<fn(f64) -> f64>,
        None,
    );
    let mut m = HashMap::new();
    m.insert("x".to_string(), Value::Float(r.x[0]).ref_cell());
    m.insert(
        "iterations".to_string(),
        Value::Int(r.nfev as i64).ref_cell(),
    );
    Ok(Value::Object(m).ref_cell())
}

fn noptim_minimize_scalar(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "noptim_minimize_scalar", span)?;
    let a = num_arg(args, 0, "noptim_minimize_scalar", span)?;
    let b = num_arg(args, 1, "noptim_minimize_scalar", span)?;
    let f = |x: f64| (x - 1.5).powi(2) + 0.1;
    let r = niao_optim::minimize_scalar(f, (a, b), ScalarMethod::Brent);
    Ok(Value::Float(r.x[0]).ref_cell())
}

macro_rules! noptim_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

noptim_fns![
    ("noptim_sqrt2_root", "sqrt2_root", noptim_sqrt2_root),
    (
        "noptim_minimize_scalar",
        "minimize_scalar",
        noptim_minimize_scalar
    ),
];

pub const MODULE_NAME: &str = "noptim";
pub const MODULE_PATHS: &[&str] = &["noptim", "std/noptim"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(f, _, fn_)| (f, fn_))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}
