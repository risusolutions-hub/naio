//! Native nonnx standard library — ONNX model loading + CPU inference
//! (~onnxruntime subset).
//!
//! Import with `import "nonnx"` (or `import "std/nonnx"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_onnx::{
    batch_from_rows, engine_version, inspect_bytes, inspect_path, load_bytes, load_path,
    tensor_f32, zeros_f32, IoDesc, OnnxError, OnnxSession, SessionOptions,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E4610: u32 = codes::E4610_NONNX_ARITY;
const E4611: u32 = codes::E4611_NONNX_ERROR;
const E4612: u32 = codes::E4612_NONNX_TYPE;
const E4613: u32 = codes::E4613_NONNX_PARAM;
const E4614: u32 = codes::E4614_NONNX_INVALID_HANDLE;
const E4615: u32 = codes::E4615_NONNX_IO;
const E4616: u32 = codes::E4616_NONNX_SHAPE;

thread_local! {
    static SESSIONS: RefCell<HashMap<i64, OnnxSession>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc(session: OnnxSession) -> i64 {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    SESSIONS.with(|m| m.borrow_mut().insert(id, session));
    id
}

fn with_session<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&OnnxSession) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    SESSIONS.with(|m| match m.borrow().get(&id) {
        Some(s) => Ok(Ok(f(s))),
        None => Ok(Err(error_value(
            E4614,
            "nonnx_error",
            format!("invalid or closed nonnx session handle {id}"),
            span,
        ))),
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4612, msg.into())
}

fn soft_err(span: Span, err: OnnxError) -> ValueRef {
    let code = match &err {
        OnnxError::Empty | OnnxError::Path(_) => E4615,
        OnnxError::InvalidHandle => E4614,
        OnnxError::Param(_) => E4613,
        OnnxError::ShapeMismatch { .. }
        | OnnxError::SizeMismatch { .. }
        | OnnxError::DtypeMismatch { .. } => E4616,
        OnnxError::MissingInput(_) | OnnxError::UnknownInput(_) | OnnxError::UnknownOutput(_) => {
            E4613
        }
        OnnxError::Engine(_) => E4611,
    };
    error_value(code, "nonnx_error", err.message(), span)
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4610,
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
            E4610,
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
        Value::Float(f) => Ok(*f as i64),
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

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a session handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects byte_array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::Object(m) => Some(m.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn session_opts(map: Option<&HashMap<String, ValueRef>>, span: Span) -> NiaoResult<SessionOptions> {
    let mut opts = SessionOptions::default();
    let Some(map) = map else {
        return Ok(opts);
    };
    if let Some(v) = map.get("threads") {
        match &*v.borrow() {
            Value::Int(n) => opts.num_threads = Some(*n as usize),
            Value::Float(f) => opts.num_threads = Some(*f as usize),
            other => {
                return Err(type_err(
                    span,
                    format!("opts.threads expects int, got {}", other.type_name()),
                ));
            }
        }
    }
    Ok(opts)
}

fn floats(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<f64>> {
    match &*args[idx].borrow() {
        Value::FloatArray(v) => Ok(v.clone()),
        Value::IntArray(v) => Ok(v.iter().map(|n| *n as f64).collect()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::Int(n) => out.push(*n as f64),
                    Value::Float(f) => out.push(*f),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects numeric array as argument {}, got {}",
                                idx + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects array/float_array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn ints_from_array(v: &ValueRef, name: &str, span: Span) -> NiaoResult<Vec<usize>> {
    match &*v.borrow() {
        Value::IntArray(v) => {
            if v.iter().any(|n| *n < 0) {
                return Err(type_err(span, format!("{name} shape entries must be >= 0")));
            }
            Ok(v.iter().map(|n| *n as usize).collect())
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::Int(n) if *n >= 0 => out.push(*n as usize),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name} shape expects non-negative ints, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!("{name} shape expects int array, got {}", other.type_name()),
        )),
    }
}

fn io_desc_obj(desc: &IoDesc) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("name".into(), Value::String(desc.name.clone()).ref_cell());
    let shape_items: Vec<ValueRef> = desc
        .shape
        .iter()
        .map(|d| match d {
            Some(n) => Value::Int(*n as i64).ref_cell(),
            None => Value::Nil.ref_cell(),
        })
        .collect();
    m.insert("shape".into(), Value::Array(shape_items).ref_cell());
    m.insert("dtype".into(), Value::String(desc.dtype.clone()).ref_cell());
    Value::Object(m).ref_cell()
}

fn io_list(descs: &[IoDesc]) -> ValueRef {
    Value::Array(descs.iter().map(io_desc_obj).collect()).ref_cell()
}

fn inspect_result(inputs: Vec<IoDesc>, outputs: Vec<IoDesc>) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("inputs".into(), io_list(&inputs));
    m.insert("outputs".into(), io_list(&outputs));
    Value::Object(m).ref_cell()
}

fn tensor_feed_from_value(v: &ValueRef, span: Span) -> NiaoResult<(Vec<usize>, Vec<f32>)> {
    match &*v.borrow() {
        Value::Object(m) => {
            let shape_v = m
                .get("shape")
                .ok_or_else(|| type_err(span, "tensor feed object requires shape field"))?;
            let data_v = m
                .get("data")
                .ok_or_else(|| type_err(span, "tensor feed object requires data field"))?;
            let shape = ints_from_array(shape_v, "tensor", span)?;
            let floats = floats(&[data_v.clone()], 0, "tensor", span)?;
            let data: Vec<f32> = floats.iter().map(|x| *x as f32).collect();
            tensor_f32(&shape, &data).map_err(|e| type_err(span, e.message()))
        }
        other => Err(type_err(
            span,
            format!(
                "tensor feed expects object {{shape, data}}, got {}",
                other.type_name()
            ),
        )),
    }
}

fn feed_from_object(
    map: &HashMap<String, ValueRef>,
    span: Span,
) -> NiaoResult<Result<HashMap<String, (Vec<usize>, Vec<f32>)>, ValueRef>> {
    let mut feed = HashMap::new();
    for (name, val) in map {
        match tensor_feed_from_value(val, span) {
            Ok(t) => {
                feed.insert(name.clone(), t);
            }
            Err(e) => return Ok(Err(error_value(E4612, "nonnx_error", e.message(), span))),
        }
    }
    if feed.is_empty() {
        return Ok(Err(error_value(
            E4613,
            "nonnx_error",
            "run() feed object must not be empty",
            span,
        )));
    }
    Ok(Ok(feed))
}

fn outputs_to_object(out: HashMap<String, (Vec<usize>, Vec<f32>)>) -> ValueRef {
    let mut m = HashMap::new();
    let mut order = Vec::new();
    for (name, (shape, data)) in out {
        let mut t = HashMap::new();
        let shape_items: Vec<ValueRef> = shape
            .iter()
            .map(|n| Value::Int(*n as i64).ref_cell())
            .collect();
        t.insert("shape".into(), Value::Array(shape_items).ref_cell());
        let data_f64: Vec<f64> = data.iter().map(|x| *x as f64).collect();
        t.insert("data".into(), Value::FloatArray(data_f64).ref_cell());
        order.push(Value::Object(t.clone()).ref_cell());
        m.insert(name, Value::Object(t).ref_cell());
    }
    m.insert("_order".into(), Value::Array(order).ref_cell());
    Value::Object(m).ref_cell()
}

fn result_at(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nonnx_output_at", span)?;
    let obj = args[0].clone();
    let map = match &*obj.borrow() {
        Value::Object(m) => m.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "nonnx.output_at() expects result object as argument 1, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let idx = int_arg(args, 1, "nonnx_output_at", span)?;
    if idx < 0 {
        return Err(type_err(span, "nonnx.output_at() index must be >= 0"));
    }
    let order = map
        .get("_order")
        .ok_or_else(|| type_err(span, "nonnx.output_at() expects a nonnx run result object"))?
        .clone();
    let order_val = order.borrow().clone();
    match order_val {
        Value::Array(items) => {
            if idx as usize >= items.len() {
                return Ok(error_value(
                    E4613,
                    "nonnx_error",
                    format!("output index {idx} out of range (len {})", items.len()),
                    span,
                ));
            }
            Ok(items[idx as usize].clone())
        }
        other => Err(type_err(
            span,
            format!(
                "nonnx.output_at() internal _order field has wrong type: {}",
                other.type_name()
            ),
        )),
    }
}

// >>> import "nonnx"; nonnx.version() != ""
fn nonnx_version(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(Value::String(engine_version().into()).ref_cell())
}

// >>> import "nonnx"; nonnx.inspect("/nonexistent/model.onnx")
fn nonnx_inspect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nonnx_inspect", span)?;
    let path = string_arg(args, 0, "nonnx_inspect", span)?;
    match inspect_path(&path) {
        Ok((inputs, outputs)) => Ok(inspect_result(inputs, outputs)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nonnx"; nonnx.inspect_bytes(io_read_bytes("Cargo.toml"))
fn nonnx_inspect_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nonnx_inspect_bytes", span)?;
    let bytes = bytes_arg(args, 0, "nonnx_inspect_bytes", span)?;
    match inspect_bytes(&bytes) {
        Ok((inputs, outputs)) => Ok(inspect_result(inputs, outputs)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nonnx"; nonnx.load("/nonexistent/model.onnx")
fn nonnx_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nonnx_load", span)?;
    let path = string_arg(args, 0, "nonnx_load", span)?;
    let opts = session_opts(optional_object(args, 1).as_ref(), span)?;
    match load_path(&path, &opts) {
        Ok(session) => Ok(Value::Int(alloc(session)).ref_cell()),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nonnx"; nonnx.load_bytes(io_read_bytes("Cargo.toml"))
fn nonnx_load_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nonnx_load_bytes", span)?;
    let bytes = bytes_arg(args, 0, "nonnx_load_bytes", span)?;
    let opts = session_opts(optional_object(args, 1).as_ref(), span)?;
    match load_bytes(&bytes, &opts) {
        Ok(session) => Ok(Value::Int(alloc(session)).ref_cell()),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nonnx"; nonnx.close(999999)
fn nonnx_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nonnx_close", span)?;
    let id = handle_arg(args, 0, "nonnx_close", span)?;
    let removed = SESSIONS.with(|m| m.borrow_mut().remove(&id).is_some());
    if removed {
        Ok(Value::Bool(true).ref_cell())
    } else {
        Ok(error_value(
            E4614,
            "nonnx_error",
            format!("invalid or closed nonnx session handle {id}"),
            span,
        ))
    }
}

// >>> import "nonnx"; nonnx.inputs(999999)
fn nonnx_inputs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nonnx_inputs", span)?;
    let id = handle_arg(args, 0, "nonnx_inputs", span)?;
    match with_session(id, span, |s| io_list(s.inputs()))? {
        Ok(v) => Ok(v),
        Err(v) => Ok(v),
    }
}

// >>> import "nonnx"; nonnx.outputs(999999)
fn nonnx_outputs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nonnx_outputs", span)?;
    let id = handle_arg(args, 0, "nonnx_outputs", span)?;
    match with_session(id, span, |s| io_list(s.outputs()))? {
        Ok(v) => Ok(v),
        Err(v) => Ok(v),
    }
}

// >>> import "nonnx"; nonnx.output_at({ _order: [] }, 0)
fn nonnx_output_at(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    result_at(args, span)
}

// >>> import "nonnx"; nonnx.run_input(999999, "x", nonnx.tensor([1], [1.0]))
fn nonnx_run_input(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nonnx_run_input", span)?;
    let id = handle_arg(args, 0, "nonnx_run_input", span)?;
    let name = string_arg(args, 1, "nonnx_run_input", span)?;
    let tensor = tensor_feed_from_value(&args[2], span)?;
    let mut feed = HashMap::new();
    feed.insert(name, tensor);
    SESSIONS.with(|m| match m.borrow().get(&id) {
        None => Ok(error_value(
            E4614,
            "nonnx_error",
            format!("invalid or closed nonnx session handle {id}"),
            span,
        )),
        Some(s) => match s.run_f32(&feed) {
            Ok(out) => Ok(outputs_to_object(out)),
            Err(e) => Ok(soft_err(span, e)),
        },
    })
}

// >>> import "nonnx"; nonnx.run(999999, {x: nonnx.tensor([1], [1.0])})
fn nonnx_run(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nonnx_run", span)?;
    let id = handle_arg(args, 0, "nonnx_run", span)?;
    let feed_map = match &*args[1].borrow() {
        Value::Object(m) => m.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "nonnx.run() expects feed object as argument 2, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    match feed_from_object(&feed_map, span)? {
        Ok(feed) => SESSIONS.with(|m| match m.borrow().get(&id) {
            None => Ok(error_value(
                E4614,
                "nonnx_error",
                format!("invalid or closed nonnx session handle {id}"),
                span,
            )),
            Some(s) => match s.run_f32(&feed) {
                Ok(out) => Ok(outputs_to_object(out)),
                Err(e) => Ok(soft_err(span, e)),
            },
        }),
        Err(v) => Ok(v),
    }
}

// >>> import "nonnx"; len(nonnx.tensor([2, 2], [1.0, 2.0, 3.0, 4.0]).shape) == 2
fn nonnx_tensor(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nonnx_tensor", span)?;
    let shape = ints_from_array(&args[0], "nonnx_tensor", span)?;
    let floats = floats(args, 1, "nonnx_tensor", span)?;
    let data: Vec<f32> = floats.iter().map(|x| *x as f32).collect();
    match tensor_f32(&shape, &data) {
        Ok((shape, data)) => {
            let mut m = HashMap::new();
            let shape_items: Vec<ValueRef> = shape
                .iter()
                .map(|n| Value::Int(*n as i64).ref_cell())
                .collect();
            m.insert("shape".into(), Value::Array(shape_items).ref_cell());
            let data_f64: Vec<f64> = data.iter().map(|x| *x as f64).collect();
            m.insert("data".into(), Value::FloatArray(data_f64).ref_cell());
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nonnx"; len(nonnx.zeros([2, 2]).shape) == 2
fn nonnx_zeros(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nonnx_zeros", span)?;
    let shape = ints_from_array(&args[0], "nonnx_zeros", span)?;
    match zeros_f32(&shape) {
        Ok((shape, data)) => {
            let mut m = HashMap::new();
            let shape_items: Vec<ValueRef> = shape
                .iter()
                .map(|n| Value::Int(*n as i64).ref_cell())
                .collect();
            m.insert("shape".into(), Value::Array(shape_items).ref_cell());
            let data_f64: Vec<f64> = data.iter().map(|x| *x as f64).collect();
            m.insert("data".into(), Value::FloatArray(data_f64).ref_cell());
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nonnx"; len(nonnx.batch([[1.0, 2.0], [3.0, 4.0]]).shape) == 2
fn nonnx_batch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nonnx_batch", span)?;
    let rows_val = match &*args[0].borrow() {
        Value::Array(items) => items.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "nonnx.batch() expects array of rows, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let mut rows = Vec::with_capacity(rows_val.len());
    for row in rows_val {
        let floats = floats(&[row], 0, "nonnx_batch", span)?;
        rows.push(floats.iter().map(|x| *x as f32).collect());
    }
    match batch_from_rows(&rows) {
        Ok((shape, data)) => {
            let mut m = HashMap::new();
            let shape_items: Vec<ValueRef> = shape
                .iter()
                .map(|n| Value::Int(*n as i64).ref_cell())
                .collect();
            m.insert("shape".into(), Value::Array(shape_items).ref_cell());
            let data_f64: Vec<f64> = data.iter().map(|x| *x as f64).collect();
            m.insert("data".into(), Value::FloatArray(data_f64).ref_cell());
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(soft_err(span, e)),
    }
}

macro_rules! nonnx_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nonnx_fns![
    ("nonnx_version", "version", nonnx_version),
    ("nonnx_inspect", "inspect", nonnx_inspect),
    ("nonnx_inspect_bytes", "inspect_bytes", nonnx_inspect_bytes),
    ("nonnx_load", "load", nonnx_load),
    ("nonnx_load_bytes", "load_bytes", nonnx_load_bytes),
    ("nonnx_close", "close", nonnx_close),
    ("nonnx_inputs", "inputs", nonnx_inputs),
    ("nonnx_outputs", "outputs", nonnx_outputs),
    ("nonnx_run", "run", nonnx_run),
    ("nonnx_run_input", "run_input", nonnx_run_input),
    ("nonnx_output_at", "output_at", nonnx_output_at),
    ("nonnx_tensor", "tensor", nonnx_tensor),
    ("nonnx_zeros", "zeros", nonnx_zeros),
    ("nonnx_batch", "batch", nonnx_batch),
];

pub const MODULE_NAME: &str = "nonnx";
pub const MODULE_PATHS: &[&str] = &["nonnx", "std/nonnx"];

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

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn version_doctest() {
        let v = nonnx_version(&[], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::String(s) if !s.is_empty()));
    }

    #[test]
    fn tensor_doctest() {
        let shape =
            Value::Array(vec![Value::Int(2).ref_cell(), Value::Int(2).ref_cell()]).ref_cell();
        let data = Value::FloatArray(vec![1.0, 2.0, 3.0, 4.0]).ref_cell();
        let t = nonnx_tensor(&[shape, data], span()).unwrap();
        match &*t.borrow() {
            Value::Object(m) => match &*m.get("shape").unwrap().borrow() {
                Value::Array(a) => assert_eq!(a.len(), 2),
                _ => panic!("expected shape array"),
            },
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn inspect_bytes_invalid_bytes() {
        let r = nonnx_inspect_bytes(&[Value::ByteArray(vec![255]).ref_cell()], span());
        let v = r.unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }
}
