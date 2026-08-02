//! Native nstats — descriptive stats, correlation, hypothesis tests.

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_stats::{mean, pearsonr, std, ttest_ind, Alternative};
use std::collections::HashMap;
use std::rc::Rc;

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E4020_NSTATS_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn floats(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<f64>> {
    match &*args[idx].borrow() {
        Value::FloatArray(v) => Ok(v.clone()),
        other => Err(RuntimeError::at(
            span,
            codes::E4022_NSTATS_TYPE,
            format!("{name}() expects float_array, got {}", other.type_name()),
        )),
    }
}

fn nstats_mean(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstats_mean", span)?;
    let v = floats(args, 0, "nstats_mean", span)?;
    let m =
        mean(&v).map_err(|e| RuntimeError::at(span, codes::E4021_NSTATS_ERROR, e.to_string()))?;
    Ok(Value::Float(m).ref_cell())
}

fn nstats_std(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstats_std", span)?;
    let v = floats(args, 0, "nstats_std", span)?;
    let s =
        std(&v, 1).map_err(|e| RuntimeError::at(span, codes::E4021_NSTATS_ERROR, e.to_string()))?;
    Ok(Value::Float(s).ref_cell())
}

fn nstats_pearsonr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstats_pearsonr", span)?;
    let x = floats(args, 0, "nstats_pearsonr", span)?;
    let y = floats(args, 1, "nstats_pearsonr", span)?;
    let r = pearsonr(&x, &y)
        .map_err(|e| RuntimeError::at(span, codes::E4021_NSTATS_ERROR, e.to_string()))?;
    let mut m = HashMap::new();
    m.insert("r".to_string(), Value::Float(r.statistic).ref_cell());
    m.insert("p".to_string(), Value::Float(r.pvalue).ref_cell());
    Ok(Value::Object(m).ref_cell())
}

fn nstats_ttest_ind(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstats_ttest_ind", span)?;
    let a = floats(args, 0, "nstats_ttest_ind", span)?;
    let b = floats(args, 1, "nstats_ttest_ind", span)?;
    let t = ttest_ind(&a, &b, false, Alternative::TwoSided)
        .map_err(|e| RuntimeError::at(span, codes::E4021_NSTATS_ERROR, e.to_string()))?;
    let mut m = HashMap::new();
    m.insert(
        "statistic".to_string(),
        Value::Float(t.statistic).ref_cell(),
    );
    m.insert("pvalue".to_string(), Value::Float(t.pvalue).ref_cell());
    Ok(Value::Object(m).ref_cell())
}

macro_rules! nstats_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nstats_fns![
    ("nstats_mean", "mean", nstats_mean),
    ("nstats_std", "std", nstats_std),
    ("nstats_pearsonr", "pearsonr", nstats_pearsonr),
    ("nstats_ttest_ind", "ttest_ind", nstats_ttest_ind),
];

pub const MODULE_NAME: &str = "nstats";
pub const MODULE_PATHS: &[&str] = &["nstats", "std/nstats"];

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
