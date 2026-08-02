//! Native nsoa standard library — columnar struct-of-arrays tables with typed
//! columns (int, float, bool, string).
//!
//! Import with `import "nsoa"` (or `import "std/nsoa"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, StringArray, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E3380_NSOA_ARITY: u32 = 3380;
const E3381_NSOA_ERROR: u32 = 3381;
const E3382_NSOA_TYPE: u32 = 3382;
const E3383_NSOA_INVALID_HANDLE: u32 = 3383;

// ---------------------------------------------------------------------------
// Column model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum ColType {
    Int,
    Float,
    Bool,
    String,
}

#[derive(Clone)]
enum Column {
    Int(Vec<i64>),
    Float(Vec<f64>),
    Bool(Vec<u8>),
    String(Vec<String>),
}

impl Column {
    fn ty(&self) -> ColType {
        match self {
            Column::Int(_) => ColType::Int,
            Column::Float(_) => ColType::Float,
            Column::Bool(_) => ColType::Bool,
            Column::String(_) => ColType::String,
        }
    }

    fn len(&self) -> usize {
        match self {
            Column::Int(v) => v.len(),
            Column::Float(v) => v.len(),
            Column::Bool(v) => v.len(),
            Column::String(v) => v.len(),
        }
    }

    fn push_value(&mut self, val: &Value, span: Span) -> Result<(), ValueRef> {
        match self {
            Column::Int(col) => match val {
                Value::Int(n) => {
                    col.push(*n);
                    Ok(())
                }
                other => Err(type_col_err(span, "int", other)),
            },
            Column::Float(col) => match val {
                Value::Int(n) => {
                    col.push(*n as f64);
                    Ok(())
                }
                Value::Float(f) => {
                    col.push(*f);
                    Ok(())
                }
                other => Err(type_col_err(span, "float", other)),
            },
            Column::Bool(col) => match val {
                Value::Bool(b) => {
                    col.push(if *b { 1 } else { 0 });
                    Ok(())
                }
                other => Err(type_col_err(span, "bool", other)),
            },
            Column::String(col) => match val {
                Value::String(s) => {
                    col.push(s.clone());
                    Ok(())
                }
                other => Err(type_col_err(span, "string", other)),
            },
        }
    }

    fn get_value(&self, row: usize) -> Option<ValueRef> {
        match self {
            Column::Int(v) => v.get(row).map(|&n| Value::Int(n).ref_cell()),
            Column::Float(v) => v.get(row).map(|&f| Value::Float(f).ref_cell()),
            Column::Bool(v) => v.get(row).map(|&b| Value::Bool(b != 0).ref_cell()),
            Column::String(v) => v.get(row).map(|s| Value::String(s.clone()).ref_cell()),
        }
    }

    fn to_array_value(&self) -> Value {
        match self {
            Column::Int(v) => Value::IntArray(v.clone()),
            Column::Float(v) => Value::FloatArray(v.clone()),
            Column::Bool(v) => Value::BoolArray(v.clone()),
            Column::String(v) => Value::StringArray(StringArray::dense(v.clone())),
        }
    }
}

fn type_col_err(span: Span, expected: &str, other: &Value) -> ValueRef {
    error_value(
        E3382_NSOA_TYPE,
        "nsoa_error",
        format!(
            "expected {expected} column value, got {}",
            other.type_name()
        ),
        span,
    )
}

struct SoaTable {
    columns: Vec<(String, Column)>,
}

impl SoaTable {
    fn new(schema: HashMap<String, ColType>) -> Result<Self, String> {
        if schema.is_empty() {
            return Err("schema must contain at least one column".into());
        }
        let mut names: Vec<String> = schema.keys().cloned().collect();
        names.sort();
        let columns = names
            .into_iter()
            .map(|name| {
                let ty = schema.get(&name).unwrap().clone();
                let col = match ty {
                    ColType::Int => Column::Int(Vec::new()),
                    ColType::Float => Column::Float(Vec::new()),
                    ColType::Bool => Column::Bool(Vec::new()),
                    ColType::String => Column::String(Vec::new()),
                };
                (name, col)
            })
            .collect();
        Ok(SoaTable { columns })
    }

    fn row_count(&self) -> usize {
        self.columns.first().map(|(_, c)| c.len()).unwrap_or(0)
    }

    fn push_row(&mut self, row: &HashMap<String, ValueRef>, span: Span) -> Result<(), ValueRef> {
        for (name, col) in &mut self.columns {
            let val = match row.get(name) {
                Some(v) => v.borrow(),
                None => {
                    return Err(nsoa_err(
                        span,
                        format!("missing column '{name}' in row object"),
                    ))
                }
            };
            col.push_value(&val, span)?;
        }
        Ok(())
    }

    fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|(n, _)| n == name).map(|(_, c)| c)
    }

    fn names(&self) -> Vec<String> {
        self.columns.iter().map(|(n, _)| n.clone()).collect()
    }
}

thread_local! {
    static TABLES: RefCell<HashMap<i64, SoaTable>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn with_table<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut SoaTable) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    TABLES.with(|tables| {
        let mut tables = tables.borrow_mut();
        match tables.get_mut(&id) {
            Some(t) => Ok(Ok(f(t))),
            None => Ok(Err(error_value(
                E3383_NSOA_INVALID_HANDLE,
                "nsoa_error",
                format!("invalid or closed nsoa handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3380_NSOA_ARITY,
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
            E3380_NSOA_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3382_NSOA_TYPE, msg.into())
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

fn object_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.iter().map(|(k, v)| (k.clone(), Rc::clone(v))).collect()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn nsoa_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3381_NSOA_ERROR, "nsoa_error", msg.into(), span)
}

fn parse_schema(
    map: &HashMap<String, ValueRef>,
    span: Span,
) -> Result<HashMap<String, ColType>, RuntimeError> {
    let mut schema = HashMap::new();
    for (name, val) in map {
        let ty = match &*val.borrow() {
            Value::String(s) => match s.as_str() {
                "int" => ColType::Int,
                "float" => ColType::Float,
                "bool" => ColType::Bool,
                "string" => ColType::String,
                other => {
                    return Err(RuntimeError::at(
                        span,
                        E3381_NSOA_ERROR,
                        format!("unknown column type '{other}' for '{name}'"),
                    ))
                }
            },
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "schema.{name} must be a type string, got {}",
                        other.type_name()
                    ),
                ));
            }
        };
        schema.insert(name.clone(), ty);
    }
    Ok(schema)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nsoa_new(schema) → handle
fn nsoa_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsoa_new", span)?;
    let schema_obj = object_arg(args, 0, "nsoa_new", span)?;
    let schema = parse_schema(&schema_obj, span)?;
    let table = match SoaTable::new(schema) {
        Ok(t) => t,
        Err(msg) => return Ok(nsoa_err(span, msg)),
    };
    let id = new_handle();
    TABLES.with(|tables| tables.borrow_mut().insert(id, table));
    Ok(Value::Int(id).ref_cell())
}

/// nsoa_close(handle) → bool
fn nsoa_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsoa_close", span)?;
    let id = int_arg(args, 0, "nsoa_close", span)?;
    let removed = TABLES.with(|tables| tables.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

/// nsoa_len(handle) → row count
fn nsoa_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsoa_len", span)?;
    let id = int_arg(args, 0, "nsoa_len", span)?;
    match with_table(id, span, |t| t.row_count())? {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nsoa_push(handle, row) → nil
fn nsoa_push(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsoa_push", span)?;
    let id = int_arg(args, 0, "nsoa_push", span)?;
    let row = object_arg(args, 1, "nsoa_push", span)?;
    match with_table(id, span, |t| t.push_row(&row, span))? {
        Ok(Ok(())) => Ok(Value::Nil.ref_cell()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsoa_column(handle, name) → packed array
fn nsoa_column(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsoa_column", span)?;
    let id = int_arg(args, 0, "nsoa_column", span)?;
    let name = string_arg(args, 1, "nsoa_column", span)?;
    match with_table(id, span, |t| t.column(&name).map(|c| c.to_array_value()))? {
        Ok(Some(v)) => Ok(v.ref_cell()),
        Ok(None) => Ok(nsoa_err(span, format!("unknown column '{name}'"))),
        Err(e) => Ok(e),
    }
}

/// nsoa_get(handle, row, col?) → value or row object
fn nsoa_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsoa_get", span)?;
    let id = int_arg(args, 0, "nsoa_get", span)?;
    let row = int_arg(args, 1, "nsoa_get", span)?;
    if row < 0 {
        return Ok(nsoa_err(span, "row index must be >= 0"));
    }
    let col = if args.len() == 3 {
        Some(string_arg(args, 2, "nsoa_get", span)?)
    } else {
        None
    };
    match with_table(id, span, |t| {
        let n = t.row_count();
        if row as usize >= n {
            return Err(nsoa_err(
                span,
                format!("row index {row} out of range (table has {n} rows)"),
            ));
        }
        if let Some(name) = col {
            match t.column(&name) {
                Some(c) => c
                    .get_value(row as usize)
                    .ok_or_else(|| nsoa_err(span, format!("column '{name}' missing value"))),
                None => Err(nsoa_err(span, format!("unknown column '{name}'"))),
            }
        } else {
            let mut obj = HashMap::new();
            for (name, col) in &t.columns {
                if let Some(v) = col.get_value(row as usize) {
                    obj.insert(name.clone(), v);
                }
            }
            Ok(Value::Object(obj).ref_cell())
        }
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsoa_names(handle) → column names
fn nsoa_names(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsoa_names", span)?;
    let id = int_arg(args, 0, "nsoa_names", span)?;
    match with_table(id, span, |t| t.names())? {
        Ok(names) => {
            let items: Vec<ValueRef> = names
                .into_iter()
                .map(|n| Value::String(n).ref_cell())
                .collect();
            Ok(Value::Array(items).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

/// nsoa_stats(handle) → {rows, columns}
fn nsoa_stats(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsoa_stats", span)?;
    let id = int_arg(args, 0, "nsoa_stats", span)?;
    match with_table(id, span, |t| {
        let rows = t.row_count();
        let cols: Vec<ValueRef> = t
            .columns
            .iter()
            .map(|(name, col)| {
                let ty = match col.ty() {
                    ColType::Int => "int",
                    ColType::Float => "float",
                    ColType::Bool => "bool",
                    ColType::String => "string",
                };
                let mut m = HashMap::new();
                m.insert("name".to_string(), Value::String(name.clone()).ref_cell());
                m.insert("type".to_string(), Value::String(ty.to_string()).ref_cell());
                m.insert("len".to_string(), Value::Int(col.len() as i64).ref_cell());
                Value::Object(m).ref_cell()
            })
            .collect();
        (rows, cols)
    })? {
        Ok((rows, cols)) => {
            let mut map = HashMap::new();
            map.insert("rows".to_string(), Value::Int(rows as i64).ref_cell());
            map.insert("columns".to_string(), Value::Array(cols).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nsoa_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nsoa_fns![
    ("nsoa_new", "new", nsoa_new),
    ("nsoa_close", "close", nsoa_close),
    ("nsoa_len", "len", nsoa_len),
    ("nsoa_push", "push", nsoa_push),
    ("nsoa_column", "column", nsoa_column),
    ("nsoa_get", "get", nsoa_get),
    ("nsoa_names", "names", nsoa_names),
    ("nsoa_stats", "stats", nsoa_stats),
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
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nsoa";
pub const MODULE_PATHS: &[&str] = &["nsoa", "std/nsoa"];

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

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn schema() -> ValueRef {
        let mut m = HashMap::new();
        m.insert("id".to_string(), Value::String("int".into()).ref_cell());
        m.insert(
            "score".to_string(),
            Value::String("float".into()).ref_cell(),
        );
        m.insert(
            "active".to_string(),
            Value::String("bool".into()).ref_cell(),
        );
        m.insert(
            "name".to_string(),
            Value::String("string".into()).ref_cell(),
        );
        Value::Object(m).ref_cell()
    }

    fn row(id: i64, score: f64, active: bool, name: &str) -> ValueRef {
        let mut m = HashMap::new();
        m.insert("id".to_string(), Value::Int(id).ref_cell());
        m.insert("score".to_string(), Value::Float(score).ref_cell());
        m.insert("active".to_string(), Value::Bool(active).ref_cell());
        m.insert("name".to_string(), Value::String(name.into()).ref_cell());
        Value::Object(m).ref_cell()
    }

    #[test]
    fn push_column_get() {
        let h = nsoa_new(&[schema()], span()).unwrap();
        nsoa_push(&[h.clone(), row(1, 9.5, true, "a")], span()).unwrap();
        nsoa_push(&[h.clone(), row(2, 8.0, false, "b")], span()).unwrap();
        let col = nsoa_column(&[h.clone(), Value::String("id".into()).ref_cell()], span()).unwrap();
        assert!(matches!(&*col.borrow(), Value::IntArray(v) if v == &[1, 2]));
        let cell = nsoa_get(
            &[h.clone(), i(0), Value::String("name".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert!(matches!(&*cell.borrow(), Value::String(s) if s == "a"));
        let row_obj = nsoa_get(&[h.clone(), i(1)], span()).unwrap();
        assert!(matches!(&*row_obj.borrow(), Value::Object(_)));
        nsoa_close(&[h], span()).unwrap();
    }
}
