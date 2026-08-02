//! Native ndataset — dataset loading, splits, shuffling, and batch iteration
//! (~HuggingFace datasets / PyTorch DataLoader subset).
//!
//! Import with `import "ndataset"` (or `import "std/ndataset"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_dataset::{
    from_row_maps, load_csv, load_json, load_jsonl, split_ratios, BatchLoader, Dataset,
};
use niao_errors::codes;
use niao_frame::{ColumnData, FilterValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E4120: u32 = codes::E4120_NDATASET_ARITY;
const E4121: u32 = codes::E4121_NDATASET_ERROR;
const E4122: u32 = codes::E4122_NDATASET_TYPE;
const E4123: u32 = codes::E4123_NDATASET_INVALID_HANDLE;
const E4124: u32 = codes::E4124_NDATASET_COLUMN;
const E4125: u32 = codes::E4125_NDATASET_INDEX;

struct LoaderStore {
    loader: BatchLoader,
    dataset_id: i64,
}

thread_local! {
    static DATASETS: RefCell<HashMap<i64, Dataset>> = RefCell::new(HashMap::new());
    static LOADERS: RefCell<HashMap<i64, LoaderStore>> = RefCell::new(HashMap::new());
    static NEXT_DS: RefCell<i64> = const { RefCell::new(1) };
    static NEXT_LOADER: RefCell<i64> = const { RefCell::new(1) };
}

fn new_dataset(ds: Dataset) -> i64 {
    let id = NEXT_DS.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    DATASETS.with(|m| m.borrow_mut().insert(id, ds));
    id
}

fn new_loader(loader: BatchLoader, dataset_id: i64) -> i64 {
    let id = NEXT_LOADER.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    LOADERS.with(|m| {
        m.borrow_mut()
            .insert(id, LoaderStore { loader, dataset_id })
    });
    id
}

fn with_dataset<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&Dataset) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    DATASETS.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(ds) => Ok(Ok(f(ds))),
            None => Ok(Err(error_value(
                E4123,
                "ndataset_error",
                format!("invalid or closed dataset handle {id}"),
                span,
            ))),
        }
    })
}

fn with_loader_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut LoaderStore) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    LOADERS.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(store) => Ok(Ok(f(store))),
            None => Ok(Err(error_value(
                E4123,
                "ndataset_error",
                format!("invalid or closed loader handle {id}"),
                span,
            ))),
        }
    })
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
            E4120,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4122, msg.into())
}

fn ds_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4121, "ndataset_error", msg.into(), span)
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        Value::Float(f) if f.fract() == 0.0 && f.is_finite() => Ok(*f as i64),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects int as argument {}, got {}",
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
                "{name}() expects number as argument {}, got {}",
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
                "{name}() expects dataset handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn loader_handle(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects loader handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Some(map.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn bool_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        Some(Value::String(s)) => matches!(s.as_str(), "true" | "1" | "yes" | "on"),
        _ => default,
    }
}

fn int_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: i64) -> i64 {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => n,
        Some(Value::Float(f)) if f.fract() == 0.0 => f as i64,
        _ => default,
    }
}

fn float_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: f64) -> f64 {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => n as f64,
        Some(Value::Float(f)) => f,
        _ => default,
    }
}

fn char_field(
    map: Option<&HashMap<String, ValueRef>>,
    key: &str,
    default: char,
    span: Span,
) -> NiaoResult<char> {
    let Some(map) = map else {
        return Ok(default);
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) if !s.is_empty() => {
            let mut chars = s.chars();
            let c = chars.next().unwrap();
            if chars.next().is_some() {
                return Err(type_err(
                    span,
                    format!("opts.{key} must be a single character"),
                ));
            }
            Ok(c)
        }
        _ => Ok(default),
    }
}

fn value_to_filter(v: &Value, span: Span) -> NiaoResult<FilterValue> {
    match v {
        Value::Int(n) => Ok(FilterValue::I64(*n)),
        Value::Float(f) => Ok(FilterValue::F64(*f)),
        Value::Bool(b) => Ok(FilterValue::Bool(*b)),
        Value::String(s) => Ok(FilterValue::Str(s.clone())),
        other => Err(type_err(
            span,
            format!(
                "filter value must be bool, number, or string; got {}",
                other.type_name()
            ),
        )),
    }
}

fn value_to_cell_string(v: &Value) -> String {
    match v {
        Value::Nil => String::new(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => {
            if f.fract() == 0.0 && f.is_finite() {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::String(s) => s.clone(),
        other => format!("<{}>", other.type_name()),
    }
}

fn row_object(ds: &Dataset, row: usize, _span: Span) -> NiaoResult<ValueRef> {
    let frame = &ds.frame;
    let mut map = HashMap::new();
    for col in &frame.columns {
        if col.validity.is_null(row) {
            map.insert(col.name.clone(), Value::Nil.ref_cell());
            continue;
        }
        let val = match &col.data {
            ColumnData::I64(v) | ColumnData::Date(v) => Value::Int(v[row]).ref_cell(),
            ColumnData::F64(v) => {
                let x = v[row];
                if x.fract() == 0.0 && x.is_finite() && x.abs() < 1e15 {
                    Value::Int(x as i64).ref_cell()
                } else {
                    Value::Float(x).ref_cell()
                }
            }
            ColumnData::Bool(v) => Value::Bool(v[row]).ref_cell(),
            ColumnData::Str(v) => Value::String(v.get(row).to_string()).ref_cell(),
        };
        map.insert(col.name.clone(), val);
    }
    Ok(Value::Object(map).ref_cell())
}

fn rows_from_indices(ds: &Dataset, indices: &[usize], span: Span) -> NiaoResult<ValueRef> {
    let mut rows = Vec::with_capacity(indices.len());
    for &i in indices {
        rows.push(row_object(ds, i, span)?);
    }
    Ok(Value::Array(rows).ref_cell())
}

fn string_array(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<String>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() column list[{}] must be string, got {}",
                                i,
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
                "{name}() expects string array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn handle_array(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<i64>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Int(id) if *id > 0 => out.push(*id),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}()[{}] must be dataset handle, got {}",
                                i,
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
                "{name}() expects handle array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

// >>> import "ndataset"
// >>> let ds = ndataset.from_rows([{x: 1, y: "a"}, {x: 2, y: "b"}])
// >>> ndataset.len(ds)
// 2
fn ndataset_from_rows(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndataset_from_rows", span)?;
    let rows_val = &*args[0].borrow();
    let Value::Array(rows) = rows_val else {
        return Err(type_err(
            span,
            format!(
                "ndataset.from_rows() expects array of objects, got {}",
                rows_val.type_name()
            ),
        ));
    };
    let mut maps: Vec<HashMap<String, String>> = Vec::with_capacity(rows.len());
    for (ri, row) in rows.iter().enumerate() {
        match &*row.borrow() {
            Value::Object(obj) => {
                let mut map = HashMap::new();
                for (k, v) in obj {
                    map.insert(k.clone(), value_to_cell_string(&v.borrow()));
                }
                maps.push(map);
            }
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "ndataset.from_rows()[{}] must be object, got {}",
                        ri,
                        other.type_name()
                    ),
                ));
            }
        }
    }
    match from_row_maps(maps) {
        Ok(ds) => Ok(Value::Int(new_dataset(ds)).ref_cell()),
        Err(e) => Ok(ds_err(span, e.to_string())),
    }
}

fn ndataset_from_csv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndataset_from_csv", span)?;
    let path = string_arg(args, 0, "ndataset_from_csv", span)?;
    let opts = optional_object(args, 1);
    let header = bool_field(opts.as_ref(), "header", true);
    let delimiter = char_field(opts.as_ref(), "delimiter", ',', span)?;
    match load_csv(&path, header, delimiter) {
        Ok(ds) => Ok(Value::Int(new_dataset(ds)).ref_cell()),
        Err(e) => Ok(ds_err(span, e.to_string())),
    }
}

fn ndataset_from_json(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndataset_from_json", span)?;
    let path = string_arg(args, 0, "ndataset_from_json", span)?;
    match load_json(&path) {
        Ok(ds) => Ok(Value::Int(new_dataset(ds)).ref_cell()),
        Err(e) => Ok(ds_err(span, e.to_string())),
    }
}

fn ndataset_from_jsonl(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndataset_from_jsonl", span)?;
    let path = string_arg(args, 0, "ndataset_from_jsonl", span)?;
    match load_jsonl(&path) {
        Ok(ds) => Ok(Value::Int(new_dataset(ds)).ref_cell()),
        Err(e) => Ok(ds_err(span, e.to_string())),
    }
}

fn ndataset_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndataset_len", span)?;
    let id = handle_arg(args, 0, "ndataset_len", span)?;
    match with_dataset(id, span, |ds| ds.len() as i64)? {
        Ok(n) => Ok(Value::Int(n).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ndataset_columns(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndataset_columns", span)?;
    let id = handle_arg(args, 0, "ndataset_columns", span)?;
    match with_dataset(id, span, |ds| {
        ds.columns()
            .into_iter()
            .map(|s| Value::String(s).ref_cell())
            .collect::<Vec<_>>()
    })? {
        Ok(cols) => Ok(Value::Array(cols).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ndataset_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "ndataset_get", span)?;
    let id = handle_arg(args, 0, "ndataset_get", span)?;
    let index = int_arg(args, 1, "ndataset_get", span)? as isize;
    match with_dataset(id, span, |ds| ds.check_index(index))? {
        Ok(Ok(row)) => {
            let ds = DATASETS.with(|m| m.borrow().get(&id).cloned());
            match ds {
                Some(ds) => row_object(&ds, row, span),
                None => Ok(error_value(
                    E4123,
                    "ndataset_error",
                    format!("invalid or closed dataset handle {id}"),
                    span,
                )),
            }
        }
        Ok(Err(e)) => Ok(error_value(E4125, "ndataset_error", e.to_string(), span)),
        Err(e) => Ok(e),
    }
}

fn ndataset_select(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "ndataset_select", span)?;
    let id = handle_arg(args, 0, "ndataset_select", span)?;
    let cols = string_array(args, 1, "ndataset_select", span)?;
    match with_dataset(id, span, |ds| ds.select(&cols))? {
        Ok(Ok(out)) => Ok(Value::Int(new_dataset(out)).ref_cell()),
        Ok(Err(e)) => Ok(error_value(E4124, "ndataset_error", e.to_string(), span)),
        Err(e) => Ok(e),
    }
}

fn ndataset_filter_eq(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 3, "ndataset_filter_eq", span)?;
    let id = handle_arg(args, 0, "ndataset_filter_eq", span)?;
    let col = string_arg(args, 1, "ndataset_filter_eq", span)?;
    let val = value_to_filter(&args[2].borrow(), span)?;
    match with_dataset(id, span, |ds| ds.filter_eq(&col, &val))? {
        Ok(Ok(out)) => Ok(Value::Int(new_dataset(out)).ref_cell()),
        Ok(Err(e)) => Ok(error_value(E4124, "ndataset_error", e.to_string(), span)),
        Err(e) => Ok(e),
    }
}

fn ndataset_shuffle(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndataset_shuffle", span)?;
    let id = handle_arg(args, 0, "ndataset_shuffle", span)?;
    let seed = if args.len() > 1 {
        int_arg(args, 1, "ndataset_shuffle", span)? as u64
    } else {
        0u64
    };
    match with_dataset(id, span, |ds| ds.shuffle(seed))? {
        Ok(Ok(out)) => Ok(Value::Int(new_dataset(out)).ref_cell()),
        Ok(Err(e)) => Ok(ds_err(span, e.to_string())),
        Err(e) => Ok(e),
    }
}

fn ndataset_split(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ndataset_split", span)?;
    let id = handle_arg(args, 0, "ndataset_split", span)?;
    let train = float_arg(args, 1, "ndataset_split", span)?;
    let opts = optional_object(args, 2);
    let val = {
        let v = float_field(opts.as_ref(), "val", -1.0);
        if v >= 0.0 {
            Some(v)
        } else {
            None
        }
    };
    let test = {
        let t = float_field(opts.as_ref(), "test", -1.0);
        if t >= 0.0 {
            Some(t)
        } else {
            None
        }
    };
    let seed = int_field(opts.as_ref(), "seed", 0) as u64;
    match with_dataset(id, span, |ds| split_ratios(ds, train, val, test, seed))? {
        Ok(Ok(parts)) => {
            let mut map = HashMap::new();
            map.insert(
                "train".to_string(),
                Value::Int(new_dataset(parts.train)).ref_cell(),
            );
            if let Some(v) = parts.val {
                map.insert("val".to_string(), Value::Int(new_dataset(v)).ref_cell());
            }
            if let Some(t) = parts.test {
                map.insert("test".to_string(), Value::Int(new_dataset(t)).ref_cell());
            }
            Ok(Value::Object(map).ref_cell())
        }
        Ok(Err(e)) => Ok(ds_err(span, e.to_string())),
        Err(e) => Ok(e),
    }
}

fn ndataset_concat(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndataset_concat", span)?;
    let ids = handle_array(args, 0, "ndataset_concat", span)?;
    if ids.is_empty() {
        return Ok(ds_err(span, "concat requires at least one dataset handle"));
    }
    let mut refs: Vec<Dataset> = Vec::with_capacity(ids.len());
    for hid in ids {
        match with_dataset(hid, span, |ds| ds.clone())? {
            Ok(ds) => refs.push(ds),
            Err(e) => return Ok(e),
        }
    }
    let ptrs: Vec<&Dataset> = refs.iter().collect();
    match Dataset::concat(&ptrs) {
        Ok(out) => Ok(Value::Int(new_dataset(out)).ref_cell()),
        Err(e) => Ok(ds_err(span, e.to_string())),
    }
}

fn ndataset_take(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "ndataset_take", span)?;
    let id = handle_arg(args, 0, "ndataset_take", span)?;
    let n = int_arg(args, 1, "ndataset_take", span)? as usize;
    match with_dataset(id, span, |ds| ds.take(n))? {
        Ok(Ok(out)) => Ok(Value::Int(new_dataset(out)).ref_cell()),
        Ok(Err(e)) => Ok(ds_err(span, e.to_string())),
        Err(e) => Ok(e),
    }
}

fn ndataset_skip(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "ndataset_skip", span)?;
    let id = handle_arg(args, 0, "ndataset_skip", span)?;
    let n = int_arg(args, 1, "ndataset_skip", span)? as usize;
    match with_dataset(id, span, |ds| ds.skip(n))? {
        Ok(Ok(out)) => Ok(Value::Int(new_dataset(out)).ref_cell()),
        Ok(Err(e)) => Ok(ds_err(span, e.to_string())),
        Err(e) => Ok(e),
    }
}

fn ndataset_to_rows(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndataset_to_rows", span)?;
    let id = handle_arg(args, 0, "ndataset_to_rows", span)?;
    match with_dataset(id, span, |ds| (0..ds.len()).collect::<Vec<usize>>())? {
        Ok(indices) => {
            let ds = DATASETS.with(|m| m.borrow().get(&id).cloned());
            match ds {
                Some(ds) => rows_from_indices(&ds, &indices, span),
                None => Ok(error_value(
                    E4123,
                    "ndataset_error",
                    format!("invalid or closed dataset handle {id}"),
                    span,
                )),
            }
        }
        Err(e) => Ok(e),
    }
}

fn ndataset_batch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ndataset_batch", span)?;
    let id = handle_arg(args, 0, "ndataset_batch", span)?;
    let batch_size = int_arg(args, 1, "ndataset_batch", span)? as usize;
    if batch_size == 0 {
        return Ok(ds_err(span, "batch_size must be >= 1"));
    }
    let opts = optional_object(args, 2);
    let shuffle = bool_field(opts.as_ref(), "shuffle", false);
    let drop_last = bool_field(opts.as_ref(), "drop_last", false);
    let seed = int_field(opts.as_ref(), "seed", 0) as u64;
    match with_dataset(id, span, |ds| {
        BatchLoader::new(ds.len(), batch_size, shuffle, seed, drop_last)
    })? {
        Ok(loader) => Ok(Value::Int(new_loader(loader, id)).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ndataset_next(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndataset_next", span)?;
    let lid = loader_handle(args, 0, "ndataset_next", span)?;
    let batch_indices = match with_loader_mut(lid, span, |store| store.loader.next_indices())? {
        Ok(Some(ix)) => ix,
        Ok(None) => return Ok(Value::Nil.ref_cell()),
        Err(e) => return Ok(e),
    };
    let ds_id = LOADERS.with(|m| m.borrow().get(&lid).unwrap().dataset_id);
    let ds = DATASETS.with(|m| m.borrow().get(&ds_id).cloned());
    let Some(ds) = ds else {
        return Ok(error_value(
            E4123,
            "ndataset_error",
            format!("dataset for loader {lid} was closed"),
            span,
        ));
    };
    rows_from_indices(&ds, &batch_indices, span)
}

fn ndataset_reset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndataset_reset", span)?;
    let lid = loader_handle(args, 0, "ndataset_reset", span)?;
    match with_loader_mut(lid, span, |store| store.loader.reset())? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ndataset_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndataset_close", span)?;
    let id = handle_arg(args, 0, "ndataset_close", span)?;
    let removed = DATASETS.with(|m| m.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

fn ndataset_close_loader(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndataset_close_loader", span)?;
    let id = loader_handle(args, 0, "ndataset_close_loader", span)?;
    let removed = LOADERS.with(|m| m.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

macro_rules! ndataset_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ndataset_fns![
    ("ndataset_from_rows", "from_rows", ndataset_from_rows),
    ("ndataset_from_csv", "from_csv", ndataset_from_csv),
    ("ndataset_from_json", "from_json", ndataset_from_json),
    ("ndataset_from_jsonl", "from_jsonl", ndataset_from_jsonl),
    ("ndataset_len", "len", ndataset_len),
    ("ndataset_columns", "columns", ndataset_columns),
    ("ndataset_get", "get", ndataset_get),
    ("ndataset_select", "select", ndataset_select),
    ("ndataset_filter_eq", "filter_eq", ndataset_filter_eq),
    ("ndataset_shuffle", "shuffle", ndataset_shuffle),
    ("ndataset_split", "split", ndataset_split),
    ("ndataset_concat", "concat", ndataset_concat),
    ("ndataset_take", "take", ndataset_take),
    ("ndataset_skip", "skip", ndataset_skip),
    ("ndataset_to_rows", "to_rows", ndataset_to_rows),
    ("ndataset_batch", "batch", ndataset_batch),
    ("ndataset_next", "next", ndataset_next),
    ("ndataset_reset", "reset", ndataset_reset),
    ("ndataset_close", "close", ndataset_close),
    (
        "ndataset_close_loader",
        "close_loader",
        ndataset_close_loader
    ),
];

pub const MODULE_NAME: &str = "ndataset";
pub const MODULE_PATHS: &[&str] = &["ndataset", "std/ndataset"];

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
        Span {
            line: 1,
            col: 1,
            start: 0,
            end: 0,
        }
    }

    #[test]
    fn from_rows_roundtrip() {
        let rows = Value::Array(vec![
            Value::Object({
                let mut m = HashMap::new();
                m.insert("a".into(), Value::Int(1).ref_cell());
                m.insert("b".into(), Value::String("x".into()).ref_cell());
                m
            })
            .ref_cell(),
            Value::Object({
                let mut m = HashMap::new();
                m.insert("a".into(), Value::Int(2).ref_cell());
                m.insert("b".into(), Value::String("y".into()).ref_cell());
                m
            })
            .ref_cell(),
        ])
        .ref_cell();
        let h = ndataset_from_rows(&[rows], span()).unwrap();
        let id = match &*h.borrow() {
            Value::Int(n) => *n,
            _ => panic!("expected handle"),
        };
        let n = ndataset_len(&[Value::Int(id).ref_cell()], span()).unwrap();
        match &*n.borrow() {
            Value::Int(count) => assert_eq!(*count, 2),
            other => panic!("expected int len, got {}", other.type_name()),
        }
    }
}
