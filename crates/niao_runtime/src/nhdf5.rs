//! Native nhdf5 standard library — HDF5 scientific dataset read/write, groups,
//! attrs (~h5py subset).
//!
//! Import with `import "nhdf5"` (or `import "std/nhdf5"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_hdf5::{
    close_file, copy_file, copy_object, create_dataset, create_file, create_group, dataset,
    dataset_dtype, dataset_shape, del_attr, flatten_data, get_attr, is_hdf5, library_version,
    link_exists, member_names, nest_data, object_kind, open_file, parallel_read, read_attrs,
    read_dataset, resize_dataset, set_attr, tree, write_dataset, CreateOpts, DynData, File, Mode,
    SliceSpec, TreeNode,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

enum H5Entry {
    File(File),
    Dataset(niao_hdf5::Dataset),
}

thread_local! {
    static STORE: RefCell<HashMap<i64, H5Entry>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc(entry: H5Entry) -> i64 {
    let id = NEXT_HANDLE.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    STORE.with(|s| s.borrow_mut().insert(id, entry));
    id
}

fn with_file<T>(id: i64, span: Span, f: impl FnOnce(&File) -> T) -> NiaoResult<Result<T, ValueRef>> {
    STORE.with(|s| {
        match s.borrow().get(&id) {
            Some(H5Entry::File(file)) => Ok(Ok(f(file))),
            Some(_) => Ok(Err(invalid_handle(span, id, "file"))),
            None => Ok(Err(invalid_handle(span, id, "file"))),
        }
    })
}

fn with_dataset<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&niao_hdf5::Dataset) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    STORE.with(|s| {
        match s.borrow().get(&id) {
            Some(H5Entry::Dataset(ds)) => Ok(Ok(f(ds))),
            Some(_) => Ok(Err(invalid_handle(span, id, "dataset"))),
            None => Ok(Err(invalid_handle(span, id, "dataset"))),
        }
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4352_NHDF5_TYPE, msg.into())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4350_NHDF5_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn hdf5_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4351_NHDF5_ERROR, "nhdf5_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64, kind: &str) -> ValueRef {
    error_value(
        codes::E4353_NHDF5_INVALID_HANDLE,
        "nhdf5_error",
        format!("invalid or closed {kind} handle {id}"),
        span,
    )
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

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a handle as argument {}, got {}",
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

fn bool_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    map.and_then(|m| m.get(key).map(|v| v.borrow().clone()))
        .map(|v| match v {
            Value::Bool(b) => b,
            Value::Int(n) => n != 0,
            _ => default,
        })
        .unwrap_or(default)
}

fn int_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: i64) -> i64 {
    map.and_then(|m| m.get(key).map(|v| v.borrow().clone()))
        .map(|v| match v {
            Value::Int(n) => n,
            _ => default,
        })
        .unwrap_or(default)
}

fn string_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<String> {
    map.and_then(|m| m.get(key)).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn usize_shape(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<usize>> {
    match &*args[idx].borrow() {
        Value::IntArray(v) => Ok(v.iter().map(|&n| n as usize).collect()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::Int(n) if *n >= 0 => out.push(*n as usize),
                    other => {
                        return Err(type_err(
                            span,
                            format!("{name}() shape elements must be non-negative ints, got {}", other.type_name()),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects shape array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn mode_from_opts(map: Option<&HashMap<String, ValueRef>>, default: &str) -> String {
    string_field(map, "mode").unwrap_or_else(|| default.to_string())
}

fn open_mode(map: Option<&HashMap<String, ValueRef>>, default: &str, span: Span) -> Result<Mode, ValueRef> {
    let m = mode_from_opts(map, default);
    Mode::parse(&m).map_err(|e| hdf5_err(span, e.message()))
}

fn create_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> CreateOpts {
    let chunk = map.and_then(|m| m.get("chunk")).and_then(|v| match &*v.borrow() {
        Value::IntArray(a) => Some(a.iter().map(|&n| n as usize).collect()),
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                if let Value::Int(n) = &*item.borrow() {
                    out.push(*n as usize);
                }
            }
            if out.is_empty() { None } else { Some(out) }
        }
        _ => None,
    });
    CreateOpts {
        dtype: string_field(map, "dtype"),
        chunk,
        deflate: map
            .and_then(|m| m.get("deflate"))
            .and_then(|v| match &*v.borrow() {
                Value::Int(n) if (0..=9).contains(n) => Some(*n as u8),
                _ => None,
            }),
        shuffle: bool_field(map, "shuffle", false),
        fill_value: map
            .and_then(|m| m.get("fill_value"))
            .and_then(|v| match &*v.borrow() {
                Value::Int(n) => Some(*n as f64),
                Value::Float(f) => Some(*f),
                _ => None,
            }),
    }
}

fn slice_from_map(map: Option<&HashMap<String, ValueRef>>, span: Span) -> Result<Option<SliceSpec>, ValueRef> {
    let Some(map) = map else {
        return Ok(None);
    };
    let start = map.get("start").and_then(|v| match &*v.borrow() {
        Value::IntArray(a) => Some(a.iter().map(|&n| n as usize).collect()),
        _ => None,
    });
    let count = map.get("count").and_then(|v| match &*v.borrow() {
        Value::IntArray(a) => Some(a.iter().map(|&n| n as usize).collect()),
        _ => None,
    });
    match (start, count) {
        (Some(s), Some(c)) => {
            let stride = map.get("stride").and_then(|v| match &*v.borrow() {
                Value::IntArray(a) => Some(a.iter().map(|&n| n as usize).collect()),
                _ => None,
            });
            SliceSpec::from_parts(s, c, stride).map(Some).map_err(|e| hdf5_err(span, e.message()))
        }
        (None, None) => Ok(None),
        _ => Err(hdf5_err(span, "slice opts require both start and count IntArrays")),
    }
}

fn dyn_to_value(data: DynData, shape: Option<&[i64]>, nested: bool) -> ValueRef {
    match data {
        DynData::I64(v) => {
            if nested {
                if let Some(sh) = shape {
                    return dyn_to_value(nest_data(DynData::I64(v), sh), None, false);
                }
            }
            Value::IntArray(v).ref_cell()
        }
        DynData::F64(v) => {
            if nested {
                if let Some(sh) = shape {
                    return dyn_to_value(nest_data(DynData::F64(v), sh), None, false);
                }
            }
            Value::FloatArray(v).ref_cell()
        }
        DynData::Bool(v) => Value::BoolArray(v).ref_cell(),
        DynData::String(v) => {
            let items: Vec<ValueRef> = v.into_iter().map(|s| Value::String(s).ref_cell()).collect();
            Value::Array(items).ref_cell()
        }
        DynData::Nested(items, sh) => {
            let out: Vec<ValueRef> = items
                .into_iter()
                .map(|item| dyn_to_value(item, None, false))
                .collect();
            if sh.len() == 1 {
                Value::Array(out).ref_cell()
            } else {
                Value::Array(out).ref_cell()
            }
        }
    }
}

fn value_to_dyn(v: &Value, span: Span) -> Result<DynData, ValueRef> {
    match v {
        Value::Int(n) => Ok(DynData::I64(vec![*n])),
        Value::Float(f) => Ok(DynData::F64(vec![*f])),
        Value::Bool(b) => Ok(DynData::Bool(vec![u8::from(*b)])),
        Value::String(s) => Ok(DynData::String(vec![s.clone()])),
        Value::IntArray(a) => Ok(DynData::I64(a.clone())),
        Value::FloatArray(a) => Ok(DynData::F64(a.clone())),
        Value::BoolArray(a) => Ok(DynData::Bool(a.clone())),
        Value::Array(items) => flatten_array_items(items, span),
        other => Err(hdf5_err(
            span,
            format!("cannot convert {} to dataset payload", other.type_name()),
        )),
    }
}

fn flatten_array_items(items: &[ValueRef], span: Span) -> Result<DynData, ValueRef> {
    if items.is_empty() {
        return Ok(DynData::F64(vec![]));
    }
    match &*items[0].borrow() {
        Value::Int(_) => {
            let mut out = Vec::new();
            for item in items {
                match value_to_dyn(&item.borrow(), span)? {
                    DynData::I64(mut v) => out.append(&mut v),
                    _ => return Err(hdf5_err(span, "mixed nested array types")),
                }
            }
            Ok(DynData::I64(out))
        }
        Value::Float(_) | Value::IntArray(_) => {
            let mut out = Vec::new();
            for item in items {
                match value_to_dyn(&item.borrow(), span)? {
                    DynData::F64(mut v) => out.append(&mut v),
                    DynData::I64(v) => out.append(&mut v.into_iter().map(|x| x as f64).collect()),
                    _ => return Err(hdf5_err(span, "mixed nested array types")),
                }
            }
            Ok(DynData::F64(out))
        }
        Value::String(_) => {
            let mut out = Vec::new();
            for item in items {
                match value_to_dyn(&item.borrow(), span)? {
                    DynData::String(mut v) => out.append(&mut v),
                    _ => return Err(hdf5_err(span, "mixed string array")),
                }
            }
            Ok(DynData::String(out))
        }
        Value::Array(_) => {
            let mut nested = Vec::new();
            for item in items {
                nested.push(value_to_dyn(&item.borrow(), span)?);
            }
            Ok(DynData::Nested(nested, vec![]))
        }
        other => Err(hdf5_err(
            span,
            format!("unsupported array element type {}", other.type_name()),
        )),
    }
}

fn tree_to_object(node: &TreeNode) -> ValueRef {
    match node {
        TreeNode::Group { children } => {
            let mut map = HashMap::new();
            for (k, v) in children {
                map.insert(k.clone(), tree_to_object(v));
            }
            let mut out = HashMap::new();
            out.insert("kind".to_string(), Value::String("group".to_string()).ref_cell());
            out.insert("children".to_string(), Value::Object(map).ref_cell());
            Value::Object(out).ref_cell()
        }
        TreeNode::Dataset { shape, dtype } => {
            let mut out = HashMap::new();
            out.insert("kind".to_string(), Value::String("dataset".to_string()).ref_cell());
            out.insert(
                "shape".to_string(),
                Value::IntArray(shape.clone()).ref_cell(),
            );
            out.insert("dtype".to_string(), Value::String(dtype.clone()).ref_cell());
            Value::Object(out).ref_cell()
        }
        TreeNode::Other => Value::String("unknown".to_string()).ref_cell(),
    }
}

fn attrs_to_object(file: &File, path: &str, span: Span) -> Result<ValueRef, ValueRef> {
    let attrs = read_attrs(file, path).map_err(|e| hdf5_err(span, e.message()))?;
    let mut map = HashMap::new();
    for (k, v) in attrs {
        map.insert(k, dyn_to_value(v, None, false));
    }
    Ok(Value::Object(map).ref_cell())
}

// >>> nhdf5.is_hdf5("data.h5")
// => true
fn nhdf5_is_hdf5(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nhdf5_is_hdf5", span)?;
    let path = string_arg(args, 0, "nhdf5_is_hdf5", span)?;
    Ok(Value::Bool(is_hdf5(&path)).ref_cell())
}

// >>> nhdf5.version()
// => "1.14.5"
fn nhdf5_version(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "nhdf5_version", span)?;
    Ok(Value::String(library_version()).ref_cell())
}

// >>> nhdf5.open("data.h5")
// => 1
fn nhdf5_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nhdf5_open", span)?;
    let path = string_arg(args, 0, "nhdf5_open", span)?;
    let mode = match open_mode(optional_object(args, 1).as_ref(), "r", span) {
        Ok(m) => m,
        Err(e) => return Ok(e),
    };
    match open_file(&path, mode) {
        Ok(f) => Ok(Value::Int(alloc(H5Entry::File(f))).ref_cell()),
        Err(e) => Ok(hdf5_err(span, e.message())),
    }
}

// >>> nhdf5.create("out.h5")
// => 1
fn nhdf5_create(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nhdf5_create", span)?;
    let path = string_arg(args, 0, "nhdf5_create", span)?;
    let mode = match open_mode(optional_object(args, 1).as_ref(), "w", span) {
        Ok(m) => m,
        Err(e) => return Ok(e),
    };
    match create_file(&path, mode) {
        Ok(f) => Ok(Value::Int(alloc(H5Entry::File(f))).ref_cell()),
        Err(e) => Ok(hdf5_err(span, e.message())),
    }
}

// >>> nhdf5.close(1)
// => nil
fn nhdf5_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nhdf5_close", span)?;
    let id = handle_arg(args, 0, "nhdf5_close", span)?;
    let entry = STORE.with(|s| s.borrow_mut().remove(&id));
    match entry {
        Some(H5Entry::File(f)) => {
            let _ = close_file(f);
            Ok(Value::Nil.ref_cell())
        }
        Some(H5Entry::Dataset(_)) => Ok(Value::Nil.ref_cell()),
        None => Ok(invalid_handle(span, id, "file")),
    }
}

// >>> nhdf5.flush(1)
// => true
fn nhdf5_flush(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nhdf5_flush", span)?;
    let id = handle_arg(args, 0, "nhdf5_flush", span)?;
    match with_file(id, span, |f| niao_hdf5::flush_file(f))? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> len(nhdf5.keys(1))
// => 3
fn nhdf5_keys(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nhdf5_keys", span)?;
    let id = handle_arg(args, 0, "nhdf5_keys", span)?;
    let path = args
        .get(1)
        .map(|v| string_arg(&[v.clone()], 0, "nhdf5_keys", span))
        .transpose()?
        .unwrap_or_default();
    match with_file(id, span, |f| member_names(f, &path))? {
        Ok(Ok(names)) => {
            let items: Vec<ValueRef> = names.into_iter().map(|s| Value::String(s).ref_cell()).collect();
            Ok(Value::Array(items).ref_cell())
        }
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.exists(1, "data")
// => true
fn nhdf5_exists(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nhdf5_exists", span)?;
    let id = handle_arg(args, 0, "nhdf5_exists", span)?;
    let name = string_arg(args, 1, "nhdf5_exists", span)?;
    let base = args
        .get(2)
        .map(|v| string_arg(&[v.clone()], 0, "nhdf5_exists", span))
        .transpose()?
        .unwrap_or_default();
    match with_file(id, span, |f| link_exists(f, &base, &name))? {
        Ok(Ok(b)) => Ok(Value::Bool(b).ref_cell()),
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.kind(1, "matrix")
// => "dataset"
fn nhdf5_kind(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nhdf5_kind", span)?;
    let id = handle_arg(args, 0, "nhdf5_kind", span)?;
    let path = string_arg(args, 1, "nhdf5_kind", span)?;
    match with_file(id, span, |f| object_kind(f, &path))? {
        Ok(Ok(k)) => Ok(Value::String(k).ref_cell()),
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.create_group(1, "run/exp")
// => true
fn nhdf5_create_group(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nhdf5_create_group", span)?;
    let id = handle_arg(args, 0, "nhdf5_create_group", span)?;
    let path = string_arg(args, 1, "nhdf5_create_group", span)?;
    match with_file(id, span, |f| create_group(f, &path))? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.create_dataset(1, "matrix", [4, 4])
// => 2
fn nhdf5_create_dataset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nhdf5_create_dataset", span)?;
    let id = handle_arg(args, 0, "nhdf5_create_dataset", span)?;
    let path = string_arg(args, 1, "nhdf5_create_dataset", span)?;
    let shape = usize_shape(args, 2, "nhdf5_create_dataset", span)?;
    let opts = create_opts_from_map(optional_object(args, 3).as_ref());
    match with_file(id, span, |f| create_dataset(f, &path, &shape, &opts))? {
        Ok(Ok(ds)) => Ok(Value::Int(alloc(H5Entry::Dataset(ds))).ref_cell()),
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.dataset(1, "matrix")
// => 2
fn nhdf5_dataset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nhdf5_dataset", span)?;
    let id = handle_arg(args, 0, "nhdf5_dataset", span)?;
    let path = string_arg(args, 1, "nhdf5_dataset", span)?;
    match with_file(id, span, |f| dataset(f, &path))? {
        Ok(Ok(ds)) => Ok(Value::Int(alloc(H5Entry::Dataset(ds))).ref_cell()),
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> len(nhdf5.read(2))
// => 16
fn nhdf5_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nhdf5_read", span)?;
    let id = handle_arg(args, 0, "nhdf5_read", span)?;
    let slice = match slice_from_map(optional_object(args, 1).as_ref(), span) {
        Ok(s) => s,
        Err(e) => return Ok(e),
    };
    let nested = bool_field(optional_object(args, 1).as_ref(), "nested", true);
    match with_dataset(id, span, |ds| {
        let shape: Vec<i64> = dataset_shape(ds);
        read_dataset(ds, slice.as_ref()).map(|data| (data, shape))
    })? {
        Ok(Ok((data, shape))) => Ok(dyn_to_value(
            data,
            if nested { Some(&shape) } else { None },
            nested,
        )),
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.write(2, [1.0, 2.0, 3.0, 4.0])
// => true
fn nhdf5_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nhdf5_write", span)?;
    let id = handle_arg(args, 0, "nhdf5_write", span)?;
    let data = match value_to_dyn(&args[1].borrow(), span) {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    let slice = match slice_from_map(optional_object(args, 2).as_ref(), span) {
        Ok(s) => s,
        Err(e) => return Ok(e),
    };
    match with_dataset(id, span, |ds| {
        let shape: Vec<usize> = dataset_shape(ds).into_iter().map(|n| n as usize).collect();
        let flat = flatten_data(&data, &shape).map_err(|e| e.message())?;
        write_dataset(ds, &flat, slice.as_ref()).map_err(|e| e.message())
    })? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(msg)) => Ok(hdf5_err(span, msg)),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.shape(2)
// => [4, 4]
fn nhdf5_shape(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nhdf5_shape", span)?;
    let id = handle_arg(args, 0, "nhdf5_shape", span)?;
    match with_dataset(id, span, dataset_shape)? {
        Ok(s) => Ok(Value::IntArray(s).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.dtype(2)
// => "f64"
fn nhdf5_dtype(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nhdf5_dtype", span)?;
    let id = handle_arg(args, 0, "nhdf5_dtype", span)?;
    match with_dataset(id, span, dataset_dtype)? {
        Ok(Ok(s)) => Ok(Value::String(s).ref_cell()),
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.resize(2, [8, 8])
// => true
fn nhdf5_resize(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nhdf5_resize", span)?;
    let id = handle_arg(args, 0, "nhdf5_resize", span)?;
    let shape = usize_shape(args, 1, "nhdf5_resize", span)?;
    match with_dataset(id, span, |ds| resize_dataset(ds, &shape))? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.attrs(1, "run")
// => {version: 1}
fn nhdf5_attrs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nhdf5_attrs", span)?;
    let id = handle_arg(args, 0, "nhdf5_attrs", span)?;
    let path = args
        .get(1)
        .map(|v| string_arg(&[v.clone()], 0, "nhdf5_attrs", span))
        .transpose()?
        .unwrap_or_default();
    match with_file(id, span, |f| attrs_to_object(f, &path, span))? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.get_attr(1, "run", "version")
// => 1
fn nhdf5_get_attr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 3, "nhdf5_get_attr", span)?;
    let id = handle_arg(args, 0, "nhdf5_get_attr", span)?;
    let path = string_arg(args, 1, "nhdf5_get_attr", span)?;
    let name = string_arg(args, 2, "nhdf5_get_attr", span)?;
    match with_file(id, span, |f| get_attr(f, &path, &name))? {
        Ok(Ok(data)) => Ok(dyn_to_value(data, None, false)),
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.set_attr(1, "", "title", "experiment")
// => true
fn nhdf5_set_attr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 4, 4, "nhdf5_set_attr", span)?;
    let id = handle_arg(args, 0, "nhdf5_set_attr", span)?;
    let path = string_arg(args, 1, "nhdf5_set_attr", span)?;
    let name = string_arg(args, 2, "nhdf5_set_attr", span)?;
    let value = match value_to_dyn(&args[3].borrow(), span) {
        Ok(v) => v,
        Err(e) => return Ok(e),
    };
    match with_file(id, span, |f| set_attr(f, &path, &name, &value))? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.del_attr(1, "run", "version")
// => true
fn nhdf5_del_attr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 3, "nhdf5_del_attr", span)?;
    let id = handle_arg(args, 0, "nhdf5_del_attr", span)?;
    let path = string_arg(args, 1, "nhdf5_del_attr", span)?;
    let name = string_arg(args, 2, "nhdf5_del_attr", span)?;
    match with_file(id, span, |f| del_attr(f, &path, &name))? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.tree(1).matrix.kind
// => "dataset"
fn nhdf5_tree(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nhdf5_tree", span)?;
    let id = handle_arg(args, 0, "nhdf5_tree", span)?;
    let path = args
        .get(1)
        .map(|v| string_arg(&[v.clone()], 0, "nhdf5_tree", span))
        .transpose()?
        .unwrap_or_default();
    let depth = int_field(optional_object(args, 2).as_ref(), "depth", 8) as usize;
    match with_file(id, span, |f| tree(f, &path, depth))? {
        Ok(Ok(nodes)) => {
            let mut map = HashMap::new();
            for (k, v) in nodes {
                map.insert(k, tree_to_object(&v));
            }
            Ok(Value::Object(map).ref_cell())
        }
        Ok(Err(e)) => Ok(hdf5_err(span, e.message())),
        Err(e) => Ok(e),
    }
}

// >>> nhdf5.copy(1, "a", 1, "b")
// => true
fn nhdf5_copy(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 4, 4, "nhdf5_copy", span)?;
    let src_id = handle_arg(args, 0, "nhdf5_copy", span)?;
    let src_path = string_arg(args, 1, "nhdf5_copy", span)?;
    let dst_id = handle_arg(args, 2, "nhdf5_copy", span)?;
    let dst_path = string_arg(args, 3, "nhdf5_copy", span)?;
    let (src_f, dst_f) = STORE.with(|s| {
        let map = s.borrow();
        let src = map.get(&src_id).and_then(|e| match e {
            H5Entry::File(f) => Some(f.clone()),
            _ => None,
        });
        let dst = map.get(&dst_id).and_then(|e| match e {
            H5Entry::File(f) => Some(f.clone()),
            _ => None,
        });
        (src, dst)
    });
    let (Some(src_f), Some(dst_f)) = (src_f, dst_f) else {
        return Ok(hdf5_err(span, "copy requires two open file handles"));
    };
    match copy_object(&src_f, &src_path, &dst_f, &dst_path) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(hdf5_err(span, e.message())),
    }
}

// >>> nhdf5.copy_file("a.h5", "b.h5")
// => true
fn nhdf5_copy_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nhdf5_copy_file", span)?;
    let src = string_arg(args, 0, "nhdf5_copy_file", span)?;
    let dst = string_arg(args, 1, "nhdf5_copy_file", span)?;
    match copy_file(&src, &dst) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(hdf5_err(span, e.message())),
    }
}

// >>> len(nhdf5.parallel_read(["a.h5", "b.h5"], "data"))
// => 2
fn nhdf5_parallel_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nhdf5_parallel_read", span)?;
    let paths = match &*args[0].borrow() {
        Value::Array(items) => items
            .iter()
            .map(|v| match &*v.borrow() {
                Value::String(s) => Ok(s.clone()),
                other => Err(type_err(
                    span,
                    format!("parallel_read paths must be strings, got {}", other.type_name()),
                )),
            })
            .collect::<NiaoResult<Vec<_>>>()?,
        other => {
            return Err(type_err(
                span,
                format!("parallel_read() expects path array, got {}", other.type_name()),
            ));
        }
    };
    let dset = string_arg(args, 1, "nhdf5_parallel_read", span)?;
    let threads = int_field(optional_object(args, 2).as_ref(), "threads", 0) as usize;
    let results = parallel_read(&paths, &dset, threads);
    let mut out = Vec::with_capacity(results.len());
    for r in results {
        match r {
            Ok(data) => out.push(dyn_to_value(data, None, false)),
            Err(e) => out.push(hdf5_err(span, e.message())),
        }
    }
    Ok(Value::Array(out).ref_cell())
}

macro_rules! nhdf5_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nhdf5_fns![
    ("nhdf5_is_hdf5", "is_hdf5", nhdf5_is_hdf5),
    ("nhdf5_version", "version", nhdf5_version),
    ("nhdf5_open", "open", nhdf5_open),
    ("nhdf5_create", "create", nhdf5_create),
    ("nhdf5_close", "close", nhdf5_close),
    ("nhdf5_flush", "flush", nhdf5_flush),
    ("nhdf5_keys", "keys", nhdf5_keys),
    ("nhdf5_exists", "exists", nhdf5_exists),
    ("nhdf5_kind", "kind", nhdf5_kind),
    ("nhdf5_create_group", "create_group", nhdf5_create_group),
    ("nhdf5_create_dataset", "create_dataset", nhdf5_create_dataset),
    ("nhdf5_dataset", "dataset", nhdf5_dataset),
    ("nhdf5_read", "read", nhdf5_read),
    ("nhdf5_write", "write", nhdf5_write),
    ("nhdf5_shape", "shape", nhdf5_shape),
    ("nhdf5_dtype", "dtype", nhdf5_dtype),
    ("nhdf5_resize", "resize", nhdf5_resize),
    ("nhdf5_attrs", "attrs", nhdf5_attrs),
    ("nhdf5_get_attr", "get_attr", nhdf5_get_attr),
    ("nhdf5_set_attr", "set_attr", nhdf5_set_attr),
    ("nhdf5_del_attr", "del_attr", nhdf5_del_attr),
    ("nhdf5_tree", "tree", nhdf5_tree),
    ("nhdf5_copy", "copy", nhdf5_copy),
    ("nhdf5_copy_file", "copy_file", nhdf5_copy_file),
    ("nhdf5_parallel_read", "parallel_read", nhdf5_parallel_read),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nhdf5";
pub const MODULE_PATHS: &[&str] = &["nhdf5", "std/nhdf5"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}
