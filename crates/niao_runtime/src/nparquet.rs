//! Native nparquet standard library — Parquet + Arrow IPC read/write,
//! nframe interop (~pyarrow subset).
//!
//! Import with `import "nparquet"` (or `import "std/nparquet"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, StringArray, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_frame::{ColumnData, DataFrame, Series, Validity};
use niao_parquet::{
    parquet_info_bytes, parquet_info_file, parquet_schema_bytes, read_ipc_bytes, read_ipc_file,
    read_parquet_bytes, read_parquet_file, validate_ipc_bytes, validate_parquet_bytes,
    write_ipc_bytes, write_ipc_file, write_parquet_bytes, write_parquet_file, ParquetError,
    ReadOptions, WriteOptions, MAX_BYTES,
};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4402_NPARQUET_TYPE, msg.into())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4400_NPARQUET_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nparquet_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4401_NPARQUET_ERROR, "nparquet_error", msg.into(), span)
}

fn nparquet_format_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4403_NPARQUET_FORMAT, "nparquet_error", msg.into(), span)
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

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects byte[] as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn object_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an object table as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object_arg(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
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
        _ => default,
    }
}

fn string_array_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<Vec<String>> {
    let map = map?;
    let arr = map.get(key)?;
    match &*arr.borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    _ => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn read_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> ReadOptions {
    ReadOptions {
        columns: string_array_field(map, "columns"),
        rows: {
            let n = int_field(map, "rows", -1);
            if n >= 0 {
                Some(n as usize)
            } else {
                None
            }
        },
    }
}

fn write_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> WriteOptions {
    let mut opts = WriteOptions::default();
    if let Some(map) = map {
        if let Some(Value::String(s)) = map.get("compression").map(|v| v.borrow().clone()) {
            if let Some(c) = WriteOptions::compression_from_str(&s) {
                opts.compression = c;
            }
        }
        let rg = int_field(Some(map), "row_group_size", opts.row_group_size as i64);
        if rg > 0 {
            opts.row_group_size = rg as usize;
        }
    }
    opts
}

fn map_parquet_err(span: Span, err: ParquetError) -> ValueRef {
    let code = match &err {
        ParquetError::Parquet(_) | ParquetError::Arrow(_) | ParquetError::Schema(_) => {
            codes::E4403_NPARQUET_FORMAT
        }
        _ => codes::E4401_NPARQUET_ERROR,
    };
    error_value(code, "nparquet_error", err.message(), span)
}

fn guard_bytes(bytes: &[u8], span: Span) -> Result<(), ValueRef> {
    if bytes.len() > MAX_BYTES {
        return Err(nparquet_err(
            span,
            format!("payload exceeds {} byte limit", MAX_BYTES),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// DataFrame ↔ Niao table object
// ---------------------------------------------------------------------------

fn is_validity_key(key: &str) -> Option<&str> {
    key.strip_suffix("__valid")
}

fn dataframe_to_object(df: &DataFrame) -> Value {
    let mut map = HashMap::new();
    for col in &df.columns {
        map.insert(col.name.clone(), series_to_value(col).ref_cell());
        if col.null_count() > 0 {
            let mask: Vec<u8> = (0..col.len())
                .map(|i| if col.validity.is_valid(i) { 1 } else { 0 })
                .collect();
            let key = format!("{}__valid", col.name);
            map.insert(key, Value::BoolArray(mask).ref_cell());
        }
    }
    Value::Object(map)
}

fn series_to_value(series: &Series) -> Value {
    match &series.data {
        ColumnData::I64(v) => Value::IntArray(v.clone()),
        ColumnData::F64(v) => Value::FloatArray(v.clone()),
        ColumnData::Bool(v) => Value::BoolArray(v.iter().map(|&b| if b { 1 } else { 0 }).collect()),
        ColumnData::Str(sc) => Value::StringArray(StringArray::dense(sc.to_vec())),
        ColumnData::Date(v) => Value::IntArray(v.clone()),
    }
}

fn dataframe_from_object(map: &HashMap<String, ValueRef>) -> Result<DataFrame, String> {
    if map.is_empty() {
        return Err("table must have at least one column".into());
    }
    let mut keys: Vec<String> = map
        .keys()
        .filter(|k| is_validity_key(k).is_none())
        .cloned()
        .collect();
    keys.sort();
    if keys.is_empty() {
        return Err("table must have at least one data column".into());
    }
    let mut columns = Vec::with_capacity(keys.len());
    for name in keys {
        let series = column_from_value(&name, &map[&name].borrow(), map)?;
        columns.push(series);
    }
    DataFrame::new(columns).map_err(|e| e.to_string())
}

fn column_from_value(
    name: &str,
    v: &Value,
    map: &HashMap<String, ValueRef>,
) -> Result<Series, String> {
    let validity_key = format!("{name}__valid");
    let validity = map
        .get(&validity_key)
        .map(|vr| validity_from_value(vr, column_len(v)?))
        .transpose()?;

    let mut series = match v {
        Value::IntArray(items) => Series::from_i64(name, items.clone()),
        Value::FloatArray(items) => Series::from_f64(name, items.clone()),
        Value::BoolArray(items) => {
            Series::from_bool(name, items.iter().map(|&b| b != 0).collect())
        }
        Value::StringArray(sa) => Series::from_str(name, &sa.dense_vec()),
        Value::Array(items) => {
            if items.iter().all(|c| matches!(&*c.borrow(), Value::Nil)) {
                Series::from_i64(name, vec![0; items.len()])
            } else {
                return Err(format!(
                    "column '{name}' array must be all nil or use a typed column"
                ));
            }
        }
        other => {
            return Err(format!(
                "column '{name}' must be a typed array, got {}",
                other.type_name()
            ));
        }
    };

    if let Some(validity) = validity {
        series = series
            .with_validity(validity)
            .map_err(|e| e.to_string())?;
    }
    Ok(series)
}

fn column_len(v: &Value) -> Result<usize, String> {
    Ok(match v {
        Value::IntArray(a) => a.len(),
        Value::FloatArray(a) => a.len(),
        Value::BoolArray(a) => a.len(),
        Value::StringArray(sa) => sa.len(),
        Value::Array(a) => a.len(),
        other => {
            return Err(format!("expected array column, got {}", other.type_name()));
        }
    })
}

fn validity_from_value(vr: &ValueRef, expected: usize) -> Result<Validity, String> {
    match &*vr.borrow() {
        Value::BoolArray(bits) => {
            if bits.len() != expected {
                return Err(format!(
                    "validity mask length {} != column length {expected}",
                    bits.len()
                ));
            }
            let mask: Vec<bool> = bits.iter().map(|&b| b != 0).collect();
            Ok(Validity::from_bools(&mask))
        }
        other => Err(format!(
            "validity mask must be bool[], got {}",
            other.type_name()
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> nparquet.rows({id: [1, 2], v: [1.0, 2.0]})
// => 2
fn nparquet_rows(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nparquet_rows", span)?;
    let map = object_arg(args, 0, "nparquet_rows", span)?;
    match dataframe_from_object(&map) {
        Ok(df) => Ok(Value::Int(df.nrows() as i64).ref_cell()),
        Err(msg) => Ok(nparquet_err(span, msg)),
    }
}

// >>> len(nparquet.columns({a: [1], b: [2]}))
// => 2
fn nparquet_columns(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nparquet_columns", span)?;
    let map = object_arg(args, 0, "nparquet_columns", span)?;
    let names: Vec<ValueRef> = map
        .keys()
        .filter(|k| is_validity_key(k).is_none())
        .map(|n| Value::String(n.clone()).ref_cell())
        .collect();
    Ok(Value::Array(names).ref_cell())
}

// >>> nparquet.encode({id: [1, 2]})
// => byte[]
fn nparquet_encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nparquet_encode", span)?;
    let map = object_arg(args, 0, "nparquet_encode", span)?;
    let opts = write_opts_from_map(optional_object_arg(args, 1).as_ref());
    match dataframe_from_object(&map) {
        Ok(df) => match write_parquet_bytes(&df, &opts) {
            Ok(bytes) => {
                if let Err(e) = guard_bytes(&bytes, span) {
                    return Ok(e);
                }
                Ok(Value::ByteArray(bytes).ref_cell())
            }
            Err(e) => Ok(map_parquet_err(span, e)),
        },
        Err(msg) => Ok(nparquet_err(span, msg)),
    }
}

// >>> nparquet.decode(bytes)
// => {id: [1, 2]}
fn nparquet_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nparquet_decode", span)?;
    let bytes = bytes_arg(args, 0, "nparquet_decode", span)?;
    if let Err(e) = guard_bytes(&bytes, span) {
        return Ok(e);
    }
    let opts = read_opts_from_map(optional_object_arg(args, 1).as_ref());
    match read_parquet_bytes(&bytes, &opts) {
        Ok(df) => Ok(dataframe_to_object(&df).ref_cell()),
        Err(e) => Ok(map_parquet_err(span, e)),
    }
}

// >>> nparquet.read_file("data.parquet")
// => table object
fn nparquet_read_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nparquet_read_file", span)?;
    let path = string_arg(args, 0, "nparquet_read_file", span)?;
    let opts = read_opts_from_map(optional_object_arg(args, 1).as_ref());
    match read_parquet_file(Path::new(&path), &opts) {
        Ok(df) => Ok(dataframe_to_object(&df).ref_cell()),
        Err(e) => Ok(map_parquet_err(span, e)),
    }
}

// >>> nparquet.write_file("out.parquet", table)
// => true
fn nparquet_write_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nparquet_write_file", span)?;
    let path = string_arg(args, 0, "nparquet_write_file", span)?;
    let map = object_arg(args, 1, "nparquet_write_file", span)?;
    let opts = write_opts_from_map(optional_object_arg(args, 2).as_ref());
    match dataframe_from_object(&map) {
        Ok(df) => match write_parquet_file(Path::new(&path), &df, &opts) {
            Ok(()) => Ok(Value::Bool(true).ref_cell()),
            Err(e) => Ok(map_parquet_err(span, e)),
        },
        Err(msg) => Ok(nparquet_err(span, msg)),
    }
}

// >>> nparquet.encode_ipc({a: [1, 2]})
// => byte[]
fn nparquet_encode_ipc(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nparquet_encode_ipc", span)?;
    let map = object_arg(args, 0, "nparquet_encode_ipc", span)?;
    match dataframe_from_object(&map) {
        Ok(df) => match write_ipc_bytes(&df) {
            Ok(bytes) => {
                if let Err(e) = guard_bytes(&bytes, span) {
                    return Ok(e);
                }
                Ok(Value::ByteArray(bytes).ref_cell())
            }
            Err(e) => Ok(map_parquet_err(span, e)),
        },
        Err(msg) => Ok(nparquet_err(span, msg)),
    }
}

// >>> nparquet.decode_ipc(bytes)
// => table
fn nparquet_decode_ipc(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nparquet_decode_ipc", span)?;
    let bytes = bytes_arg(args, 0, "nparquet_decode_ipc", span)?;
    if let Err(e) = guard_bytes(&bytes, span) {
        return Ok(e);
    }
    let opts = read_opts_from_map(optional_object_arg(args, 1).as_ref());
    match read_ipc_bytes(&bytes, &opts) {
        Ok(df) => Ok(dataframe_to_object(&df).ref_cell()),
        Err(e) => Ok(map_parquet_err(span, e)),
    }
}

// >>> nparquet.read_ipc_file("data.arrow")
fn nparquet_read_ipc_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nparquet_read_ipc_file", span)?;
    let path = string_arg(args, 0, "nparquet_read_ipc_file", span)?;
    let opts = read_opts_from_map(optional_object_arg(args, 1).as_ref());
    match read_ipc_file(Path::new(&path), &opts) {
        Ok(df) => Ok(dataframe_to_object(&df).ref_cell()),
        Err(e) => Ok(map_parquet_err(span, e)),
    }
}

// >>> nparquet.write_ipc_file("out.arrow", table)
fn nparquet_write_ipc_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nparquet_write_ipc_file", span)?;
    let path = string_arg(args, 0, "nparquet_write_ipc_file", span)?;
    let map = object_arg(args, 1, "nparquet_write_ipc_file", span)?;
    match dataframe_from_object(&map) {
        Ok(df) => match write_ipc_file(Path::new(&path), &df) {
            Ok(()) => Ok(Value::Bool(true).ref_cell()),
            Err(e) => Ok(map_parquet_err(span, e)),
        },
        Err(msg) => Ok(nparquet_err(span, msg)),
    }
}

// >>> nparquet.schema(bytes)
fn nparquet_schema(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nparquet_schema", span)?;
    match &*args[0].borrow() {
        Value::ByteArray(bytes) => {
            if let Err(e) = guard_bytes(bytes, span) {
                return Ok(e);
            }
            match parquet_schema_bytes(bytes) {
                Ok(fields) => Ok(schema_object(&fields).ref_cell()),
                Err(e) => Ok(map_parquet_err(span, e)),
            }
        }
        Value::String(path) => {
            let bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(e) => return Ok(map_parquet_err(span, ParquetError::Io(e.to_string()))),
            };
            match parquet_schema_bytes(&bytes) {
                Ok(fields) => Ok(schema_object(&fields).ref_cell()),
                Err(e) => Ok(map_parquet_err(span, e)),
            }
        }
        other => Err(type_err(
            span,
            format!(
                "nparquet_schema() expects path string or byte[], got {}",
                other.type_name()
            ),
        )),
    }
}

fn schema_object(fields: &[(String, String)]) -> Value {
    let mut map = HashMap::new();
    let names: Vec<ValueRef> = fields
        .iter()
        .map(|(n, _)| Value::String(n.clone()).ref_cell())
        .collect();
    let types: Vec<ValueRef> = fields
        .iter()
        .map(|(_, t)| Value::String(t.clone()).ref_cell())
        .collect();
    map.insert("columns".to_string(), Value::Array(names).ref_cell());
    map.insert("types".to_string(), Value::Array(types).ref_cell());
    map.insert("cols".to_string(), Value::Int(fields.len() as i64).ref_cell());
    Value::Object(map)
}

// >>> nparquet.info(bytes)
fn nparquet_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nparquet_info", span)?;
    match &*args[0].borrow() {
        Value::ByteArray(bytes) => {
            if let Err(e) = guard_bytes(bytes, span) {
                return Ok(e);
            }
            match parquet_info_bytes(bytes) {
                Ok(info) => Ok(info_to_value(&info).ref_cell()),
                Err(e) => Ok(map_parquet_err(span, e)),
            }
        }
        Value::String(path) => match parquet_info_file(Path::new(path)) {
            Ok(info) => Ok(info_to_value(&info).ref_cell()),
            Err(e) => Ok(map_parquet_err(span, e)),
        },
        other => Err(type_err(
            span,
            format!(
                "nparquet_info() expects path string or byte[], got {}",
                other.type_name()
            ),
        )),
    }
}

fn info_to_value(info: &niao_parquet::ParquetInfo) -> Value {
    let mut map = HashMap::new();
    map.insert("format".to_string(), Value::String(info.format.clone()).ref_cell());
    map.insert("rows".to_string(), Value::Int(info.rows as i64).ref_cell());
    map.insert("cols".to_string(), Value::Int(info.cols as i64).ref_cell());
    map.insert(
        "columns".to_string(),
        Value::Array(
            info.columns
                .iter()
                .map(|c| Value::String(c.clone()).ref_cell())
                .collect(),
        )
        .ref_cell(),
    );
    map.insert(
        "types".to_string(),
        Value::Array(
            info.types
                .iter()
                .map(|t| Value::String(t.clone()).ref_cell())
                .collect(),
        )
        .ref_cell(),
    );
    map.insert(
        "row_groups".to_string(),
        Value::Int(info.row_groups as i64).ref_cell(),
    );
    map.insert(
        "compressed_size".to_string(),
        Value::Int(info.compressed_size as i64).ref_cell(),
    );
    map.insert(
        "uncompressed_size".to_string(),
        Value::Int(info.uncompressed_size as i64).ref_cell(),
    );
    Value::Object(map)
}

// >>> nparquet.validate(bytes)
// => true
fn nparquet_validate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nparquet_validate", span)?;
    let bytes = bytes_arg(args, 0, "nparquet_validate", span)?;
    Ok(Value::Bool(validate_parquet_bytes(&bytes)).ref_cell())
}

// >>> nparquet.validate_ipc(bytes)
fn nparquet_validate_ipc(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nparquet_validate_ipc", span)?;
    let bytes = bytes_arg(args, 0, "nparquet_validate_ipc", span)?;
    Ok(Value::Bool(validate_ipc_bytes(&bytes)).ref_cell())
}

// >>> nparquet.to_nframe(table)
fn nparquet_to_nframe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nparquet_to_nframe", span)?;
    let map = object_arg(args, 0, "nparquet_to_nframe", span)?;
    match dataframe_from_object(&map) {
        Ok(df) => Ok(Value::Int(super::nframe::store_frame(df) as i64).ref_cell()),
        Err(msg) => Ok(nparquet_err(span, msg)),
    }
}

// >>> nparquet.from_nframe(handle)
fn nparquet_from_nframe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nparquet_from_nframe", span)?;
    let id = match &*args[0].borrow() {
        Value::Int(n) if *n > 0 => *n as u64,
        other => {
            return Err(type_err(
                span,
                format!(
                    "nparquet_from_nframe() expects frame handle, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    match super::nframe::clone_frame(id) {
        Some(df) => Ok(dataframe_to_object(&df).ref_cell()),
        None => Ok(nparquet_err(span, "invalid frame handle")),
    }
}

// >>> nparquet.load("data.parquet")
fn nparquet_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nparquet_read_file(args, span)
}

// >>> nparquet.save("out.parquet", table)
fn nparquet_save(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nparquet_write_file(args, span)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nparquet_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nparquet_fns![
    ("nparquet_rows", "rows", nparquet_rows),
    ("nparquet_columns", "columns", nparquet_columns),
    ("nparquet_encode", "encode", nparquet_encode),
    ("nparquet_decode", "decode", nparquet_decode),
    ("nparquet_read_file", "read_file", nparquet_read_file),
    ("nparquet_write_file", "write_file", nparquet_write_file),
    ("nparquet_encode_ipc", "encode_ipc", nparquet_encode_ipc),
    ("nparquet_decode_ipc", "decode_ipc", nparquet_decode_ipc),
    ("nparquet_read_ipc_file", "read_ipc_file", nparquet_read_ipc_file),
    ("nparquet_write_ipc_file", "write_ipc_file", nparquet_write_ipc_file),
    ("nparquet_schema", "schema", nparquet_schema),
    ("nparquet_info", "info", nparquet_info),
    ("nparquet_validate", "validate", nparquet_validate),
    ("nparquet_validate_ipc", "validate_ipc", nparquet_validate_ipc),
    ("nparquet_to_nframe", "to_nframe", nparquet_to_nframe),
    ("nparquet_from_nframe", "from_nframe", nparquet_from_nframe),
    ("nparquet_load", "load", nparquet_load),
    ("nparquet_save", "save", nparquet_save),
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

pub const MODULE_NAME: &str = "nparquet";
pub const MODULE_PATHS: &[&str] = &["nparquet", "std/nparquet"];

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

    fn sample_table() -> ValueRef {
        let mut map = HashMap::new();
        map.insert("id".to_string(), Value::IntArray(vec![1, 2, 3]).ref_cell());
        map.insert(
            "score".to_string(),
            Value::FloatArray(vec![0.1, 0.2, 0.3]).ref_cell(),
        );
        map.insert(
            "name".to_string(),
            Value::StringArray(StringArray::dense(vec![
                "a".into(),
                "b".into(),
                "c".into(),
            ]))
            .ref_cell(),
        );
        Value::Object(map).ref_cell()
    }

    #[test]
    fn parquet_roundtrip() {
        let table = sample_table();
        let bytes = nparquet_encode(&[table.clone()], span()).unwrap();
        let restored = nparquet_decode(&[bytes], span()).unwrap();
        assert!(crate::values_equal(&table.borrow(), &restored.borrow()));
    }

    #[test]
    fn ipc_roundtrip() {
        let table = sample_table();
        let bytes = nparquet_encode_ipc(&[table.clone()], span()).unwrap();
        let restored = nparquet_decode_ipc(&[bytes], span()).unwrap();
        assert!(crate::values_equal(&table.borrow(), &restored.borrow()));
    }

    #[test]
    fn info_reports_shape() {
        let table = sample_table();
        let bytes = nparquet_encode(&[table], span()).unwrap();
        let info = nparquet_info(&[bytes], span()).unwrap();
        let ir = info.borrow().clone();
        match ir {
            Value::Object(map) => {
                assert!(matches!(&*map.get("rows").unwrap().borrow(), Value::Int(3)));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
