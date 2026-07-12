//! Native nnum standard library — n-dimensional arrays, linear algebra, FFT.
//! Import with `import "nnum"` (or `import "std/nnum"`).

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_num::{self, NdArray, NumError};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

thread_local! {
    static HANDLES: RefCell<HashMap<u64, NdArray>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<u64> = RefCell::new(1);
}

fn alloc_handle(a: NdArray) -> u64 {
    let id = NEXT_HANDLE.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    HANDLES.with(|h| h.borrow_mut().insert(id, a));
    id
}

fn with_handle<T>(
    id: u64,
    name: &str,
    span: Span,
    f: impl FnOnce(&NdArray) -> Result<T, NumError>,
) -> NiaoResult<T> {
    HANDLES.with(|h| {
        let map = h.borrow();
        let arr = map.get(&id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4001_NNUM_ERROR,
                format!("{name}(): invalid array handle {id}"),
            )
        })?;
        f(arr).map_err(num_err(span))
    })
}

fn num_err(span: Span) -> impl Fn(NumError) -> RuntimeError {
    move |e: NumError| {
        let code = match &e {
            NumError::Arity { .. } => codes::E4000_NNUM_ARITY,
            NumError::Type(_) => codes::E4002_NNUM_TYPE,
            NumError::ShapeMismatch(_) => codes::E4003_NNUM_SHAPE,
            NumError::Singular(_) => codes::E4004_NNUM_SINGULAR,
            NumError::NonConvergence(_) => codes::E4005_NNUM_NON_CONVERGENCE,
            NumError::Error(_) => codes::E4001_NNUM_ERROR,
        };
        RuntimeError::at(span, code, e.to_string())
    }
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E4000_NNUM_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4000_NNUM_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
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
            codes::E4002_NNUM_TYPE,
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
        other => Err(RuntimeError::at(
            span,
            codes::E4002_NNUM_TYPE,
            format!(
                "{name}() expects an int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn shape_from_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<usize>> {
    match &*args[idx].borrow() {
        Value::IntArray(v) => Ok(v.iter().map(|&x| x as usize).collect()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::Int(n) => out.push(*n as usize),
                    other => {
                        return Err(RuntimeError::at(
                            span,
                            codes::E4002_NNUM_TYPE,
                            format!(
                                "{name}() shape elements must be int, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(RuntimeError::at(
            span,
            codes::E4002_NNUM_TYPE,
            format!(
                "{name}() expects int_array shape, got {}",
                other.type_name()
            ),
        )),
    }
}

fn floats_from_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<f64>> {
    match &*args[idx].borrow() {
        Value::FloatArray(v) => Ok(v.clone()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(num_from_value(&*item.borrow(), name, span)?);
            }
            Ok(out)
        }
        other => Err(RuntimeError::at(
            span,
            codes::E4002_NNUM_TYPE,
            format!(
                "{name}() expects float_array data, got {}",
                other.type_name()
            ),
        )),
    }
}

fn num_from_value(v: &Value, name: &str, span: Span) -> NiaoResult<f64> {
    match v {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(RuntimeError::at(
            span,
            codes::E4002_NNUM_TYPE,
            format!("{name}(): expected number, got {}", other.type_name()),
        )),
    }
}

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<u64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id as u64),
        other => Err(RuntimeError::at(
            span,
            codes::E4002_NNUM_TYPE,
            format!(
                "{name}() expects array handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn ok_handle(id: u64) -> ValueRef {
    Value::Int(id as i64).ref_cell()
}

fn ok_float(f: f64) -> ValueRef {
    Value::Float(f).ref_cell()
}

fn ok_float_array(v: Vec<f64>) -> ValueRef {
    Value::FloatArray(v).ref_cell()
}

fn ok_int_array(v: Vec<i64>) -> ValueRef {
    Value::IntArray(v).ref_cell()
}

fn nnum_array(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nnum_array", span)?;
    let shape = shape_from_arg(args, 0, "nnum_array", span)?;
    let data = floats_from_arg(args, 1, "nnum_array", span)?;
    let a = NdArray::from_vec(shape, data).map_err(num_err(span))?;
    Ok(ok_handle(alloc_handle(a)))
}

fn nnum_zeros(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nnum_zeros", span)?;
    let shape = shape_from_arg(args, 0, "nnum_zeros", span)?;
    let a = NdArray::zeros(&shape).map_err(num_err(span))?;
    Ok(ok_handle(alloc_handle(a)))
}

fn nnum_ones(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nnum_ones", span)?;
    let shape = shape_from_arg(args, 0, "nnum_ones", span)?;
    let a = NdArray::ones(&shape).map_err(num_err(span))?;
    Ok(ok_handle(alloc_handle(a)))
}

fn nnum_linspace(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nnum_linspace", span)?;
    let start = num_arg(args, 0, "nnum_linspace", span)?;
    let stop = num_arg(args, 1, "nnum_linspace", span)?;
    let n = int_arg(args, 2, "nnum_linspace", span)? as usize;
    let a = niao_num::linspace(start, stop, n).map_err(num_err(span))?;
    Ok(ok_handle(alloc_handle(a)))
}

fn nnum_arange(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nnum_arange", span)?;
    let start = num_arg(args, 0, "nnum_arange", span)?;
    let stop = num_arg(args, 1, "nnum_arange", span)?;
    let step = if args.len() == 3 {
        num_arg(args, 2, "nnum_arange", span)?
    } else {
        1.0
    };
    let a = niao_num::arange(start, stop, step).map_err(num_err(span))?;
    Ok(ok_handle(alloc_handle(a)))
}

fn nnum_eye(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nnum_eye", span)?;
    let n = int_arg(args, 0, "nnum_eye", span)? as usize;
    let a = niao_num::eye(n).map_err(num_err(span))?;
    Ok(ok_handle(alloc_handle(a)))
}

fn nnum_shape(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nnum_shape", span)?;
    let id = handle_arg(args, 0, "nnum_shape", span)?;
    with_handle(id, "nnum_shape", span, |a| {
        Ok(a.shape.iter().map(|&d| d as i64).collect::<Vec<_>>())
    })
    .map(ok_int_array)
}

fn nnum_to_float_array(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nnum_to_float_array", span)?;
    let id = handle_arg(args, 0, "nnum_to_float_array", span)?;
    with_handle(id, "nnum_to_float_array", span, |a| Ok(a.to_vec())).map(ok_float_array)
}

fn nnum_matmul(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nnum_matmul", span)?;
    let a_id = handle_arg(args, 0, "nnum_matmul", span)?;
    let b_id = handle_arg(args, 1, "nnum_matmul", span)?;
    let a = HANDLES.with(|h| h.borrow().get(&a_id).cloned().ok_or_else(|| {
        RuntimeError::at(span, codes::E4001_NNUM_ERROR, "invalid handle")
    }))?;
    let b = HANDLES.with(|h| h.borrow().get(&b_id).cloned().ok_or_else(|| {
        RuntimeError::at(span, codes::E4001_NNUM_ERROR, "invalid handle")
    }))?;
    let out = niao_num::matmul(&a, &b).map_err(num_err(span))?;
    Ok(ok_handle(alloc_handle(out)))
}

fn nnum_sum(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nnum_sum", span)?;
    let id = handle_arg(args, 0, "nnum_sum", span)?;
    let axis = if args.len() == 2 {
        Some(int_arg(args, 1, "nnum_sum", span)? as usize)
    } else {
        None
    };
    let a = HANDLES.with(|h| h.borrow().get(&id).cloned().ok_or_else(|| {
        RuntimeError::at(span, codes::E4001_NNUM_ERROR, "invalid handle")
    }))?;
    let out = niao_num::sum(&a, axis).map_err(num_err(span))?;
    Ok(ok_handle(alloc_handle(out)))
}

fn nnum_inv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nnum_inv", span)?;
    let id = handle_arg(args, 0, "nnum_inv", span)?;
    let a = HANDLES.with(|h| h.borrow().get(&id).cloned().ok_or_else(|| {
        RuntimeError::at(span, codes::E4001_NNUM_ERROR, "invalid handle")
    }))?;
    let out = niao_num::inv(&a).map_err(num_err(span))?;
    Ok(ok_handle(alloc_handle(out)))
}

fn nnum_mean(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nnum_mean", span)?;
    let id = handle_arg(args, 0, "nnum_mean", span)?;
    let axis = if args.len() == 2 {
        Some(int_arg(args, 1, "nnum_mean", span)? as usize)
    } else {
        None
    };
    let a = HANDLES.with(|h| h.borrow().get(&id).cloned().ok_or_else(|| {
        RuntimeError::at(span, codes::E4001_NNUM_ERROR, "invalid handle")
    }))?;
    let out = niao_num::mean(&a, axis).map_err(num_err(span))?;
    Ok(ok_handle(alloc_handle(out)))
}

fn nnum_solve(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nnum_solve", span)?;
    let a_id = handle_arg(args, 0, "nnum_solve", span)?;
    let b_id = handle_arg(args, 1, "nnum_solve", span)?;
    let a = HANDLES.with(|h| h.borrow().get(&a_id).cloned().ok_or_else(|| {
        RuntimeError::at(span, codes::E4001_NNUM_ERROR, "invalid handle")
    }))?;
    let b = HANDLES.with(|h| h.borrow().get(&b_id).cloned().ok_or_else(|| {
        RuntimeError::at(span, codes::E4001_NNUM_ERROR, "invalid handle")
    }))?;
    let out = niao_num::solve(&a, &b).map_err(num_err(span))?;
    Ok(ok_handle(alloc_handle(out)))
}

fn nnum_det(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nnum_det", span)?;
    let id = handle_arg(args, 0, "nnum_det", span)?;
    with_handle(id, "nnum_det", span, |a| niao_num::det(a)).map(ok_float)
}

fn nnum_fft(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nnum_fft", span)?;
    let id = handle_arg(args, 0, "nnum_fft", span)?;
    with_handle(id, "nnum_fft", span, |a| {
        let spec = niao_num::fft(a)?;
        let re: Vec<f64> = spec.iter().map(|c| c.re).collect();
        let im: Vec<f64> = spec.iter().map(|c| c.im).collect();
        Ok((re, im))
    })
    .map(|(re, im)| {
        let mut m = HashMap::new();
        m.insert("re".to_string(), Value::FloatArray(re).ref_cell());
        m.insert("im".to_string(), Value::FloatArray(im).ref_cell());
        Value::Object(m).ref_cell()
    })
}

fn nnum_dot(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nnum_dot", span)?;
    let a_id = handle_arg(args, 0, "nnum_dot", span)?;
    let b_id = handle_arg(args, 1, "nnum_dot", span)?;
    let a = HANDLES.with(|h| h.borrow().get(&a_id).cloned().ok_or_else(|| {
        RuntimeError::at(span, codes::E4001_NNUM_ERROR, "invalid handle")
    }))?;
    let b = HANDLES.with(|h| h.borrow().get(&b_id).cloned().ok_or_else(|| {
        RuntimeError::at(span, codes::E4001_NNUM_ERROR, "invalid handle")
    }))?;
    let d = niao_num::dot(&a, &b).map_err(num_err(span))?;
    Ok(ok_float(d))
}

fn nnum_transpose(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nnum_transpose", span)?;
    let id = handle_arg(args, 0, "nnum_transpose", span)?;
    let a = HANDLES.with(|h| h.borrow().get(&id).cloned().ok_or_else(|| {
        RuntimeError::at(span, codes::E4001_NNUM_ERROR, "invalid handle")
    }))?;
    let out = a.transpose().map_err(num_err(span))?;
    Ok(ok_handle(alloc_handle(out)))
}

fn nnum_trace(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nnum_trace", span)?;
    let id = handle_arg(args, 0, "nnum_trace", span)?;
    with_handle(id, "nnum_trace", span, |a| niao_num::trace(a)).map(ok_float)
}

fn nnum_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nnum_add", span)?;
    let a_id = handle_arg(args, 0, "nnum_add", span)?;
    let b_id = handle_arg(args, 1, "nnum_add", span)?;
    let a = HANDLES.with(|h| h.borrow().get(&a_id).cloned().ok_or_else(|| {
        RuntimeError::at(span, codes::E4001_NNUM_ERROR, "invalid handle")
    }))?;
    let b = HANDLES.with(|h| h.borrow().get(&b_id).cloned().ok_or_else(|| {
        RuntimeError::at(span, codes::E4001_NNUM_ERROR, "invalid handle")
    }))?;
    let out = niao_num::add(&a, &b).map_err(num_err(span))?;
    Ok(ok_handle(alloc_handle(out)))
}

macro_rules! nnum_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nnum_fns![
    ("nnum_array", "array", nnum_array),
    ("nnum_zeros", "zeros", nnum_zeros),
    ("nnum_ones", "ones", nnum_ones),
    ("nnum_linspace", "linspace", nnum_linspace),
    ("nnum_arange", "arange", nnum_arange),
    ("nnum_eye", "eye", nnum_eye),
    ("nnum_shape", "shape", nnum_shape),
    ("nnum_to_float_array", "to_float_array", nnum_to_float_array),
    ("nnum_matmul", "matmul", nnum_matmul),
    ("nnum_sum", "sum", nnum_sum),
    ("nnum_mean", "mean", nnum_mean),
    ("nnum_solve", "solve", nnum_solve),
    ("nnum_det", "det", nnum_det),
    ("nnum_inv", "inv", nnum_inv),
    ("nnum_fft", "fft", nnum_fft),
    ("nnum_dot", "dot", nnum_dot),
    ("nnum_transpose", "transpose", nnum_transpose),
    ("nnum_trace", "trace", nnum_trace),
    ("nnum_add", "add", nnum_add),
];

pub const MODULE_NAME: &str = "nnum";
pub const MODULE_PATHS: &[&str] = &["nnum", "std/nnum"];

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
