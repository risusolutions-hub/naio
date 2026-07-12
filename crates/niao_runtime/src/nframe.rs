//! Native nframe — DataFrame read/write and shape introspection.

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_frame::{read_csv, CsvOptions, DataFrame};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

thread_local! {
    static FRAMES: RefCell<HashMap<u64, DataFrame>> = RefCell::new(HashMap::new());
    static NEXT_FRAME: RefCell<u64> = RefCell::new(1);
}

fn alloc_frame(df: DataFrame) -> u64 {
    let id = NEXT_FRAME.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    FRAMES.with(|h| h.borrow_mut().insert(id, df));
    id
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E4010_NFRAME_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(RuntimeError::at(
            span,
            codes::E4012_NFRAME_TYPE,
            format!("{name}() expects string, got {}", other.type_name()),
        )),
    }
}

fn handle(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<u64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id as u64),
        other => Err(RuntimeError::at(
            span,
            codes::E4012_NFRAME_TYPE,
            format!("{name}() expects frame handle, got {}", other.type_name()),
        )),
    }
}

fn nframe_read_csv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nframe_read_csv", span)?;
    let path = string_arg(args, 0, "nframe_read_csv", span)?;
    let df = read_csv(&path, CsvOptions::default())
        .map_err(|e| RuntimeError::at(span, codes::E4011_NFRAME_ERROR, e.to_string()))?;
    Ok(Value::Int(alloc_frame(df) as i64).ref_cell())
}

fn nframe_shape(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nframe_shape", span)?;
    let id = handle(args, 0, "nframe_shape", span)?;
    FRAMES.with(|h| {
        let map = h.borrow();
        let df = map.get(&id).ok_or_else(|| {
            RuntimeError::at(span, codes::E4011_NFRAME_ERROR, "invalid frame handle")
        })?;
        Ok(Value::IntArray(vec![df.nrows() as i64, df.ncols() as i64]).ref_cell())
    })
}

fn nframe_columns(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nframe_columns", span)?;
    let id = handle(args, 0, "nframe_columns", span)?;
    FRAMES.with(|h| {
        let map = h.borrow();
        let df = map.get(&id).ok_or_else(|| {
            RuntimeError::at(span, codes::E4011_NFRAME_ERROR, "invalid frame handle")
        })?;
        let cols: Vec<ValueRef> = df
            .column_names()
            .iter()
            .map(|s| Value::String(s.clone()).ref_cell())
            .collect();
        Ok(Value::Array(cols).ref_cell())
    })
}

macro_rules! nframe_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nframe_fns![
    ("nframe_read_csv", "read_csv", nframe_read_csv),
    ("nframe_shape", "shape", nframe_shape),
    ("nframe_columns", "columns", nframe_columns),
];

pub const MODULE_NAME: &str = "nframe";
pub const MODULE_PATHS: &[&str] = &["nframe", "std/nframe"];

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
