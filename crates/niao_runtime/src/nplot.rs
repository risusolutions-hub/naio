//! Native nplot — SVG chart rendering.

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_plot::line;
use std::collections::HashMap;
use std::rc::Rc;

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E4040_NPLOT_ARITY,
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
            codes::E4042_NPLOT_TYPE,
            format!("{name}() expects float_array, got {}", other.type_name()),
        )),
    }
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(RuntimeError::at(
            span,
            codes::E4042_NPLOT_TYPE,
            format!("{name}() expects string, got {}", other.type_name()),
        )),
    }
}

fn nplot_line(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nplot_line", span)?;
    let x = floats(args, 0, "nplot_line", span)?;
    let y = floats(args, 1, "nplot_line", span)?;
    let fig = line(&x, &y).map_err(|e| RuntimeError::at(span, codes::E4044_NPLOT_RENDER, e.to_string()))?;
    Ok(Value::String(fig.to_svg_string()).ref_cell())
}

fn nplot_save_line(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nplot_save_line", span)?;
    let x = floats(args, 0, "nplot_save_line", span)?;
    let y = floats(args, 1, "nplot_save_line", span)?;
    let path = string_arg(args, 2, "nplot_save_line", span)?;
    let fig = line(&x, &y).map_err(|e| RuntimeError::at(span, codes::E4044_NPLOT_RENDER, e.to_string()))?;
    fig.save_svg(&path).map_err(|e| RuntimeError::at(span, codes::E4044_NPLOT_RENDER, e.to_string()))?;
    Ok(Value::Nil.ref_cell())
}

macro_rules! nplot_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nplot_fns![
    ("nplot_line", "line", nplot_line),
    ("nplot_save_line", "save_line", nplot_save_line),
];

pub const MODULE_NAME: &str = "nplot";
pub const MODULE_PATHS: &[&str] = &["nplot", "std/nplot"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(f, _, fn_)| (f, fn_)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}
