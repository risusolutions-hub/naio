//! Native nxlsx standard library — Excel .xlsx read/write (~openpyxl / xlsxwriter subset).
//!
//! Import with `import "nxlsx"` (or `import "std/nxlsx"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, StringArray, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_frame::DataFrame;
use niao_xlsx::{
    column_index, column_letter, dataframe_to_table, info_file, open_file, parse_range,
    read_chunk_file, sheet_to_row_arrays, sheet_to_table, table_to_dataframe, validate_bytes,
    validate_file, write_bytes, write_file, write_table_to_sheet, CellStyle, CellValue,
    ChunkReadOptions, ReadOptions, SheetSelector, StreamStore, Table, WorkbookData, WorkbookStore,
    WriteOptions, MAX_BYTES,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

thread_local! {
    static BOOKS: RefCell<WorkbookStore> = RefCell::new(WorkbookStore::new());
    static STREAMS: RefCell<StreamStore> = RefCell::new(StreamStore::new());
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4412_NXLSX_TYPE, msg.into())
}

fn nxlsx_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4411_NXLSX_ERROR, "nxlsx_error", msg.into(), span)
}

fn nxlsx_format_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4413_NXLSX_FORMAT, "nxlsx_error", msg.into(), span)
}

fn invalid_handle(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4414_NXLSX_INVALID_HANDLE, "nxlsx_error", msg.into(), span)
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4410_NXLSX_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
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

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<u64> {
    let id = int_arg(args, idx, name, span)?;
    if id <= 0 {
        return Err(type_err(span, format!("{name}() expects a positive workbook handle")));
    }
    Ok(id as u64)
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

fn string_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<String> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        Some(Value::Int(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn read_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> ReadOptions {
    let sheet = string_field(map, "sheet").map(SheetSelector::Name).or_else(|| {
        let idx = int_field(map, "sheet_index", 0);
        if idx > 0 {
            Some(SheetSelector::Index(idx as usize))
        } else {
            None
        }
    });
    ReadOptions {
        header: bool_field(map, "header", true),
        start_row: int_field(map, "start_row", 1).max(1) as u32,
        rows: {
            let n = int_field(map, "rows", -1);
            if n >= 0 {
                Some(n as usize)
            } else {
                None
            }
        },
        sheet,
        columns: None,
        skip_empty: bool_field(map, "skip_empty", false),
        infer_types: bool_field(map, "infer_types", true),
    }
}

fn write_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> WriteOptions {
    WriteOptions {
        constant_memory: bool_field(map, "constant_memory", false),
        default_sheet: string_field(map, "sheet"),
        header: bool_field(map, "header", true),
        autofit: bool_field(map, "autofit", false),
        freeze_row: {
            let n = int_field(map, "freeze_row", 0);
            if n > 0 {
                Some(n as u32)
            } else {
                None
            }
        },
        freeze_col: {
            let n = int_field(map, "freeze_col", 0);
            if n > 0 {
                Some(n as u32)
            } else {
                None
            }
        },
    }
}

fn map_xlsx_err(span: Span, err: niao_xlsx::XlsxError) -> ValueRef {
    let code = match err {
        niao_xlsx::XlsxError::Format(_) => codes::E4413_NXLSX_FORMAT,
        _ => codes::E4411_NXLSX_ERROR,
    };
    error_value(code, "nxlsx_error", err.message(), span)
}

fn guard_bytes(bytes: &[u8], span: Span) -> Result<(), ValueRef> {
    if bytes.len() > MAX_BYTES {
        return Err(nxlsx_err(
            span,
            format!("payload exceeds {MAX_BYTES} byte limit"),
        ));
    }
    Ok(())
}

fn is_validity_key(k: &str) -> Option<&str> {
    k.strip_suffix("__valid")
}

fn table_from_object(map: &HashMap<String, ValueRef>) -> Result<Table, String> {
    if map.is_empty() {
        return Err("table must have at least one column".into());
    }
    let mut keys: Vec<String> = map
        .keys()
        .filter(|k| is_validity_key(k).is_none())
        .cloned()
        .collect();
    keys.sort();
    let mut columns = HashMap::new();
    for name in keys {
        let cells = column_cells_from_value(&name, &map[&name].borrow(), map)?;
        columns.insert(name, cells);
    }
    Table::from_columns(columns).map_err(|e| e.message())
}

fn column_cells_from_value(
    name: &str,
    v: &Value,
    map: &HashMap<String, ValueRef>,
) -> Result<Vec<CellValue>, String> {
    let validity_key = format!("{name}__valid");
    let validity = map
        .get(&validity_key)
        .map(|vr| validity_from_value(vr, column_len(v)?))
        .transpose()?;

    let mut out = match v {
        Value::IntArray(items) => items.iter().map(|&n| CellValue::Int(n)).collect(),
        Value::FloatArray(items) => items.iter().map(|&f| CellValue::Float(f)).collect(),
        Value::BoolArray(items) => items
            .iter()
            .map(|&b| CellValue::Bool(b != 0))
            .collect(),
        Value::StringArray(sa) => sa
            .dense_vec()
            .into_iter()
            .map(CellValue::String)
            .collect(),
        Value::Array(items) => items
            .iter()
            .map(|c| niao_value_to_cell(&c.borrow()))
            .collect(),
        other => {
            return Err(format!(
                "column '{name}' must be a typed array, got {}",
                other.type_name()
            ));
        }
    };

    if let Some(validity) = validity {
        for (i, ok) in validity.iter().enumerate() {
            if !ok && i < out.len() {
                out[i] = CellValue::Empty;
            }
        }
    }
    Ok(out)
}

fn column_len(v: &Value) -> Result<usize, String> {
    Ok(match v {
        Value::IntArray(a) => a.len(),
        Value::FloatArray(a) => a.len(),
        Value::BoolArray(a) => a.len(),
        Value::StringArray(sa) => sa.len(),
        Value::Array(a) => a.len(),
        other => return Err(format!("expected array column, got {}", other.type_name())),
    })
}

fn validity_from_value(vr: &ValueRef, expected: usize) -> Result<Vec<bool>, String> {
    match &*vr.borrow() {
        Value::BoolArray(bits) => {
            if bits.len() != expected {
                return Err(format!(
                    "validity mask length {} != column length {expected}",
                    bits.len()
                ));
            }
            Ok(bits.iter().map(|&b| b != 0).collect())
        }
        other => Err(format!(
            "validity mask must be bool[], got {}",
            other.type_name()
        )),
    }
}

fn niao_value_to_cell(v: &Value) -> CellValue {
    match v {
        Value::Nil => CellValue::Empty,
        Value::Int(n) => CellValue::Int(*n),
        Value::Float(f) => CellValue::Float(*f),
        Value::Bool(b) => CellValue::Bool(*b),
        Value::String(s) => CellValue::String(s.clone()),
        other => CellValue::String(other.to_string()),
    }
}

fn cell_to_value(c: &CellValue) -> ValueRef {
    match c {
        CellValue::Empty => Value::Nil.ref_cell(),
        CellValue::Int(n) => Value::Int(*n).ref_cell(),
        CellValue::Float(f) => Value::Float(*f).ref_cell(),
        CellValue::Bool(b) => Value::Bool(*b).ref_cell(),
        CellValue::String(s) | CellValue::Formula(s) | CellValue::Error(s) => {
            Value::String(s.clone()).ref_cell()
        }
        CellValue::Date(d) => Value::Float(*d).ref_cell(),
    }
}

fn table_to_object(table: &Table) -> Value {
    let mut map = HashMap::new();
    for name in table.column_names() {
        let col = table.columns.get(&name).unwrap();
        let (vals, mask): (Vec<ValueRef>, Vec<i64>) = col
            .iter()
            .map(|c| {
                if c.is_empty() {
                    (Value::Nil.ref_cell(), 0i64)
                } else {
                    (cell_to_value(c), 1i64)
                }
            })
            .unzip();
        if mask.iter().any(|&m| m == 0) {
            map.insert(format!("{name}__valid"), Value::BoolArray(mask).ref_cell());
        }
        let typed = cells_to_typed_array(col);
        map.insert(name, typed.ref_cell());
    }
    Value::Object(map)
}

fn cells_to_typed_array(col: &[CellValue]) -> Value {
    let all_int = col.iter().all(|c| matches!(c, CellValue::Int(_) | CellValue::Empty));
    let all_float = col
        .iter()
        .all(|c| matches!(c, CellValue::Int(_) | CellValue::Float(_) | CellValue::Empty));
    let all_bool = col.iter().all(|c| matches!(c, CellValue::Bool(_) | CellValue::Empty));
    if all_bool && col.iter().any(|c| matches!(c, CellValue::Bool(_))) {
        return Value::BoolArray(col.iter().map(|c| match c {
            CellValue::Bool(b) => if *b { 1 } else { 0 },
            _ => 0,
        }).collect());
    }
    if all_int && col.iter().any(|c| matches!(c, CellValue::Int(_))) {
        return Value::IntArray(col.iter().map(|c| match c {
            CellValue::Int(n) => *n,
            _ => 0,
        }).collect());
    }
    if all_float && col.iter().any(|c| matches!(c, CellValue::Float(_) | CellValue::Int(_))) {
        return Value::FloatArray(col.iter().map(|c| match c {
            CellValue::Float(f) => *f,
            CellValue::Int(n) => *n as f64,
            _ => 0.0,
        }).collect());
    }
    let strings: Vec<String> = col.iter().map(|c| c.as_display_string()).collect();
    Value::StringArray(StringArray::dense(strings))
}

fn rows_to_array(rows: &[Vec<CellValue>]) -> Value {
    let outer: Vec<ValueRef> = rows
        .iter()
        .map(|row| {
            let inner: Vec<ValueRef> = row.iter().map(cell_to_value).collect();
            Value::Array(inner).ref_cell()
        })
        .collect();
    Value::Array(outer)
}

fn style_from_map(map: &HashMap<String, ValueRef>) -> CellStyle {
    CellStyle {
        bold: bool_field(Some(map), "bold", false),
        italic: bool_field(Some(map), "italic", false),
        underline: bool_field(Some(map), "underline", false),
        font_size: {
            let n = int_field(Some(map), "font_size", 0);
            if n > 0 {
                Some(n as f64)
            } else {
                None
            }
        },
        font_color: string_field(Some(map), "font_color"),
        bg_color: string_field(Some(map), "bg_color"),
        number_format: string_field(Some(map), "number_format"),
        align: string_field(Some(map), "align"),
        valign: string_field(Some(map), "valign"),
        wrap: bool_field(Some(map), "wrap", false),
        border: string_field(Some(map), "border"),
    }
}

fn sheet_name_arg(args: &[ValueRef], idx: usize, wb: &WorkbookData, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        Value::Int(n) if *n > 0 => {
            let i = (*n - 1) as usize;
            wb.sheets
                .get(i)
                .map(|s| s.name.clone())
                .ok_or_else(|| type_err(span, format!("sheet index out of range: {n}")))
        }
        other => Err(type_err(
            span,
            format!("expected sheet name or index, got {}", other.type_name()),
        )),
    }
}

fn cell_values_from_array(v: &Value) -> Result<Vec<CellValue>, String> {
    match v {
        Value::Array(items) => items.iter().map(|c| Ok(niao_value_to_cell(&c.borrow()))).collect(),
        other => Err(format!("expected row array, got {}", other.type_name())),
    }
}

// >>> nxlsx.create()
fn nxlsx_create(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nxlsx_create", span)?;
    let _opts = write_opts_from_map(optional_object_arg(args, 0).as_ref());
    let id = BOOKS.with(|store| store.borrow_mut().alloc(WorkbookData::new()));
    Ok(Value::Int(id as i64).ref_cell())
}

// >>> nxlsx.open("book.xlsx")
fn nxlsx_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nxlsx_open", span)?;
    let path = string_arg(args, 0, "nxlsx_open", span)?;
    let opts = read_opts_from_map(optional_object_arg(args, 1).as_ref());
    match open_file(Path::new(&path), &opts) {
        Ok(wb) => {
            let id = BOOKS.with(|store| store.borrow_mut().alloc(wb));
            Ok(Value::Int(id as i64).ref_cell())
        }
        Err(e) => Ok(map_xlsx_err(span, e)),
    }
}

// >>> nxlsx.close(handle)
fn nxlsx_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nxlsx_close", span)?;
    let id = handle_arg(args, 0, "nxlsx_close", span)?;
    let closed = BOOKS.with(|store| store.borrow_mut().close(id));
    Ok(Value::Bool(closed).ref_cell())
}

// >>> nxlsx.save(handle, "out.xlsx")
fn nxlsx_save(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nxlsx_save", span)?;
    let id = handle_arg(args, 0, "nxlsx_save", span)?;
    let opts = WriteOptions::default();
    let path = if args.len() >= 2 {
        string_arg(args, 1, "nxlsx_save", span)?
    } else {
        BOOKS.with(|store| {
            store
                .borrow()
                .get(id)
                .ok()
                .and_then(|b| b.source_path.clone())
        })
        .ok_or_else(|| type_err(span, "save() requires path when workbook has no source"))?
    };
    let result = BOOKS.with(|store| {
        let book = store.borrow().get(id).map_err(|e| e.message())?;
        write_file(Path::new(&path), book, &opts).map_err(|e| e.message())
    });
    match result {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(msg) => Ok(nxlsx_err(span, msg)),
    }
}

// >>> nxlsx.to_bytes(handle)
fn nxlsx_to_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nxlsx_to_bytes", span)?;
    let id = handle_arg(args, 0, "nxlsx_to_bytes", span)?;
    let opts = write_opts_from_map(optional_object_arg(args, 1).as_ref());
    let result = BOOKS.with(|store| {
        let book = store.borrow().get(id).map_err(|e| e.message())?;
        write_bytes(book, &opts).map_err(|e| e.message())
    });
    match result {
        Ok(bytes) => {
            if let Err(e) = guard_bytes(&bytes, span) {
                return Ok(e);
            }
            Ok(Value::ByteArray(bytes).ref_cell())
        }
        Err(msg) => Ok(nxlsx_err(span, msg)),
    }
}

// >>> nxlsx.sheet_names(handle)
fn nxlsx_sheet_names(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nxlsx_sheet_names", span)?;
    let id = handle_arg(args, 0, "nxlsx_sheet_names", span)?;
    BOOKS.with(|store| match store.borrow().get(id) {
        Ok(wb) => {
            let names: Vec<ValueRef> = wb
                .sheets
                .iter()
                .map(|s| Value::String(s.name.clone()).ref_cell())
                .collect();
            Ok(Value::Array(names).ref_cell())
        }
        Err(e) => Ok(invalid_handle(span, e.message())),
    })
}

// >>> nxlsx.active_sheet(handle)
fn nxlsx_active_sheet(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nxlsx_active_sheet", span)?;
    let id = handle_arg(args, 0, "nxlsx_active_sheet", span)?;
    BOOKS.with(|store| match store.borrow().get(id) {
        Ok(wb) => Ok(Value::String(wb.sheets[wb.active].name.clone()).ref_cell()),
        Err(e) => Ok(invalid_handle(span, e.message())),
    })
}

// >>> nxlsx.set_active(handle, "Sheet2")
fn nxlsx_set_active(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nxlsx_set_active", span)?;
    let id = handle_arg(args, 0, "nxlsx_set_active", span)?;
    BOOKS.with(|store| {
        let mut s = store.borrow_mut();
        let wb = match s.get_mut(id) {
            Ok(w) => w,
            Err(e) => return Ok(invalid_handle(span, e.message())),
        };
        let name = sheet_name_arg(args, 1, wb, span)?;
        let idx = wb
            .sheet_index(&name)
            .ok_or_else(|| nxlsx_err(span, format!("sheet not found: {name}")));
        match idx {
            Ok(i) => {
                wb.active = i;
                Ok(Value::Bool(true).ref_cell())
            }
            Err(v) => Ok(v),
        }
    })
}

// >>> nxlsx.add_sheet(handle, "New")
fn nxlsx_add_sheet(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nxlsx_add_sheet", span)?;
    let id = handle_arg(args, 0, "nxlsx_add_sheet", span)?;
    let name = string_arg(args, 1, "nxlsx_add_sheet", span)?;
    BOOKS.with(|store| {
        match store.borrow_mut().get_mut(id) {
            Ok(wb) => match wb.add_sheet(&name) {
                Ok(()) => Ok(Value::Bool(true).ref_cell()),
                Err(e) => Ok(nxlsx_err(span, e.message())),
            },
            Err(e) => Ok(invalid_handle(span, e.message())),
        }
    })
}

// >>> nxlsx.remove_sheet(handle, "Old")
fn nxlsx_remove_sheet(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nxlsx_remove_sheet", span)?;
    let id = handle_arg(args, 0, "nxlsx_remove_sheet", span)?;
    let name = string_arg(args, 1, "nxlsx_remove_sheet", span)?;
    BOOKS.with(|store| match store.borrow_mut().get_mut(id) {
        Ok(wb) => match wb.remove_sheet(&name) {
            Ok(()) => Ok(Value::Bool(true).ref_cell()),
            Err(e) => Ok(nxlsx_err(span, e.message())),
        },
        Err(e) => Ok(invalid_handle(span, e.message())),
    })
}

// >>> nxlsx.rename_sheet(handle, "A", "B")
fn nxlsx_rename_sheet(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 3, "nxlsx_rename_sheet", span)?;
    let id = handle_arg(args, 0, "nxlsx_rename_sheet", span)?;
    let old = string_arg(args, 1, "nxlsx_rename_sheet", span)?;
    let new = string_arg(args, 2, "nxlsx_rename_sheet", span)?;
    BOOKS.with(|store| match store.borrow_mut().get_mut(id) {
        Ok(wb) => match wb.rename_sheet(&old, &new) {
            Ok(()) => Ok(Value::Bool(true).ref_cell()),
            Err(e) => Ok(nxlsx_err(span, e.message())),
        },
        Err(e) => Ok(invalid_handle(span, e.message())),
    })
}

// >>> nxlsx.read("book.xlsx")
fn nxlsx_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nxlsx_read", span)?;
    let path = string_arg(args, 0, "nxlsx_read", span)?;
    let opts = read_opts_from_map(optional_object_arg(args, 1).as_ref());
    match open_file(Path::new(&path), &opts) {
        Ok(wb) => {
            let mut out = HashMap::new();
            for sheet in &wb.sheets {
                match sheet_to_table(sheet, opts.header, opts.infer_types) {
                    Ok(table) => {
                        out.insert(sheet.name.clone(), table_to_object(&table).ref_cell());
                    }
                    Err(e) => return Ok(nxlsx_err(span, e.message())),
                }
            }
            Ok(Value::Object(out).ref_cell())
        }
        Err(e) => Ok(map_xlsx_err(span, e)),
    }
}

// >>> nxlsx.read_sheet(handle, "Data")
fn nxlsx_read_sheet(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nxlsx_read_sheet", span)?;
    let id = handle_arg(args, 0, "nxlsx_read_sheet", span)?;
    let opts = read_opts_from_map(optional_object_arg(args, 2).as_ref());
    BOOKS.with(|store| {
        let wb = match store.borrow().get(id) {
            Ok(w) => w,
            Err(e) => return Ok(invalid_handle(span, e.message())),
        };
        let name = sheet_name_arg(args, 1, wb, span)?;
        let sheet = match wb.sheet(&name) {
            Ok(s) => s,
            Err(e) => return Ok(nxlsx_err(span, e.message())),
        };
        match sheet_to_table(sheet, opts.header, opts.infer_types) {
            Ok(table) => Ok(table_to_object(&table).ref_cell()),
            Err(e) => Ok(nxlsx_err(span, e.message())),
        }
    })
}

// >>> nxlsx.read_rows(handle, "Data")
fn nxlsx_read_rows(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nxlsx_read_rows", span)?;
    let id = handle_arg(args, 0, "nxlsx_read_rows", span)?;
    BOOKS.with(|store| {
        let wb = match store.borrow().get(id) {
            Ok(w) => w,
            Err(e) => return Ok(invalid_handle(span, e.message())),
        };
        let name = sheet_name_arg(args, 1, wb, span)?;
        match wb.sheet(&name) {
            Ok(sheet) => Ok(rows_to_array(&sheet_to_row_arrays(sheet)).ref_cell()),
            Err(e) => Ok(nxlsx_err(span, e.message())),
        }
    })
}

// >>> nxlsx.read_chunk("book.xlsx", "Data", {start_row: 1, count: 100})
fn nxlsx_read_chunk(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nxlsx_read_chunk", span)?;
    let path = string_arg(args, 0, "nxlsx_read_chunk", span)?;
    let sheet = if args.len() >= 2 {
        Some(SheetSelector::Name(string_arg(args, 1, "nxlsx_read_chunk", span)?))
    } else {
        None
    };
    let map = optional_object_arg(args, 2);
    let opts = ChunkReadOptions {
        start_row: int_field(map.as_ref(), "start_row", 1).max(1) as u32,
        count: int_field(map.as_ref(), "count", 1000).max(1) as usize,
        sheet,
    };
    match read_chunk_file(Path::new(&path), &opts) {
        Ok(rows) => Ok(rows_to_array(&rows).ref_cell()),
        Err(e) => Ok(map_xlsx_err(span, e)),
    }
}

// >>> nxlsx.write("out.xlsx", {Sheet1: table})
fn nxlsx_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nxlsx_write", span)?;
    let path = string_arg(args, 0, "nxlsx_write", span)?;
    let sheets_map = object_arg(args, 1, "nxlsx_write", span)?;
    let opts = write_opts_from_map(optional_object_arg(args, 2).as_ref());
    let mut wb = WorkbookData::new();
    wb.sheets.clear();
    for (name, table_ref) in sheets_map {
        let table_map = match &*table_ref.borrow() {
            Value::Object(m) => m.clone(),
            other => {
                return Ok(nxlsx_err(
                    span,
                    format!(
                        "sheet '{name}' value must be table object, got {}",
                        other.type_name()
                    ),
                ));
            }
        };
        match table_from_object(&table_map) {
            Ok(table) => {
                if let Err(e) = write_table_to_sheet(&mut wb, &name, &table, opts.header) {
                    return Ok(nxlsx_err(span, e.message()));
                }
            }
            Err(msg) => return Ok(nxlsx_err(span, msg)),
        }
    }
    if wb.sheets.is_empty() {
        return Ok(nxlsx_err(span, "write() requires at least one sheet"));
    }
    match write_file(Path::new(&path), &wb, &opts) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_xlsx_err(span, e)),
    }
}

// >>> nxlsx.write_sheet(handle, "Data", table)
fn nxlsx_write_sheet(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nxlsx_write_sheet", span)?;
    let id = handle_arg(args, 0, "nxlsx_write_sheet", span)?;
    let name = string_arg(args, 1, "nxlsx_write_sheet", span)?;
    let table_map = object_arg(args, 2, "nxlsx_write_sheet", span)?;
    let header = bool_field(optional_object_arg(args, 3).as_ref(), "header", true);
    match table_from_object(&table_map) {
        Ok(table) => BOOKS.with(|store| {
            match store.borrow_mut().get_mut(id) {
                Ok(wb) => match write_table_to_sheet(wb, &name, &table, header) {
                    Ok(()) => Ok(Value::Bool(true).ref_cell()),
                    Err(e) => Ok(nxlsx_err(span, e.message())),
                },
                Err(e) => Ok(invalid_handle(span, e.message())),
            }
        }),
        Err(msg) => Ok(nxlsx_err(span, msg)),
    }
}

// >>> nxlsx.set_cell(handle, "Sheet1", 1, 1, "hello")
fn nxlsx_set_cell(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 5, 6, "nxlsx_set_cell", span)?;
    let id = handle_arg(args, 0, "nxlsx_set_cell", span)?;
    let row = int_arg(args, 2, "nxlsx_set_cell", span)? as u32;
    let col = int_arg(args, 3, "nxlsx_set_cell", span)? as u32;
    let cell = niao_value_to_cell(&*args[4].borrow());
    BOOKS.with(|store| {
        let wb = match store.borrow_mut().get_mut(id) {
            Ok(w) => w,
            Err(e) => return Ok(invalid_handle(span, e.message())),
        };
        let sheet_name = sheet_name_arg(args, 1, wb, span)?;
        match wb.sheet_mut(&sheet_name) {
            Ok(sheet) => match sheet.set_cell(row, col, cell) {
                Ok(()) => {
                    if args.len() >= 6 {
                        if let Value::Object(style_map) = &*args[5].borrow() {
                            sheet.styles.insert((row, col), style_from_map(style_map));
                        }
                    }
                    wb.dirty = true;
                    Ok(Value::Bool(true).ref_cell())
                }
                Err(e) => Ok(nxlsx_err(span, e.message())),
            },
            Err(e) => Ok(nxlsx_err(span, e.message())),
        }
    })
}

// >>> nxlsx.cell(handle, "Sheet1", 1, 1)
fn nxlsx_cell(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 4, 4, "nxlsx_cell", span)?;
    let id = handle_arg(args, 0, "nxlsx_cell", span)?;
    let row = int_arg(args, 2, "nxlsx_cell", span)? as u32;
    let col = int_arg(args, 3, "nxlsx_cell", span)? as u32;
    BOOKS.with(|store| {
        let wb = match store.borrow().get(id) {
            Ok(w) => w,
            Err(e) => return Ok(invalid_handle(span, e.message())),
        };
        let sheet_name = sheet_name_arg(args, 1, wb, span)?;
        match wb.sheet(&sheet_name) {
            Ok(sheet) => {
                if let Some(f) = sheet.formulas.get(&(row, col)) {
                    Ok(Value::String(format!("={f}")).ref_cell())
                } else {
                    Ok(cell_to_value(&sheet.get_cell(row, col)))
                }
            }
            Err(e) => Ok(nxlsx_err(span, e.message())),
        }
    })
}

// >>> nxlsx.formula(handle, "Sheet1", 2, 3, "SUM(A1:A10)")
fn nxlsx_formula(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 5, 5, "nxlsx_formula", span)?;
    let id = handle_arg(args, 0, "nxlsx_formula", span)?;
    let row = int_arg(args, 2, "nxlsx_formula", span)? as u32;
    let col = int_arg(args, 3, "nxlsx_formula", span)? as u32;
    let formula = string_arg(args, 4, "nxlsx_formula", span)?;
    BOOKS.with(|store| {
        let wb = match store.borrow_mut().get_mut(id) {
            Ok(w) => w,
            Err(e) => return Ok(invalid_handle(span, e.message())),
        };
        let sheet_name = sheet_name_arg(args, 1, wb, span)?;
        match wb.sheet_mut(&sheet_name) {
            Ok(sheet) => match sheet.set_formula(row, col, formula) {
                Ok(()) => {
                    wb.dirty = true;
                    Ok(Value::Bool(true).ref_cell())
                }
                Err(e) => Ok(nxlsx_err(span, e.message())),
            },
            Err(e) => Ok(nxlsx_err(span, e.message())),
        }
    })
}

// >>> nxlsx.style(handle, "Sheet1", "A1:C3", {bold: true})
fn nxlsx_style(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nxlsx_style", span)?;
    let id = handle_arg(args, 0, "nxlsx_style", span)?;
    let range_spec = string_arg(args, 2, "nxlsx_style", span)?;
    let style_map = if args.len() >= 4 {
        object_arg(args, 3, "nxlsx_style", span)?
    } else {
        HashMap::new()
    };
    let range = match parse_range(&range_spec) {
        Ok(r) => r,
        Err(e) => return Ok(nxlsx_err(span, e.message())),
    };
    let style = style_from_map(&style_map);
    BOOKS.with(|store| {
        let wb = match store.borrow_mut().get_mut(id) {
            Ok(w) => w,
            Err(e) => return Ok(invalid_handle(span, e.message())),
        };
        let sheet_name = sheet_name_arg(args, 1, wb, span)?;
        match wb.apply_style_range(&sheet_name, &range, style) {
            Ok(()) => Ok(Value::Bool(true).ref_cell()),
            Err(e) => Ok(nxlsx_err(span, e.message())),
        }
    })
}

// >>> nxlsx.merge(handle, "Sheet1", "A1:B2")
fn nxlsx_merge(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 3, "nxlsx_merge", span)?;
    let id = handle_arg(args, 0, "nxlsx_merge", span)?;
    let range_spec = string_arg(args, 2, "nxlsx_merge", span)?;
    let range = match parse_range(&range_spec) {
        Ok(r) => r,
        Err(e) => return Ok(nxlsx_err(span, e.message())),
    };
    BOOKS.with(|store| {
        let wb = match store.borrow_mut().get_mut(id) {
            Ok(w) => w,
            Err(e) => return Ok(invalid_handle(span, e.message())),
        };
        let sheet_name = sheet_name_arg(args, 1, wb, span)?;
        match wb.apply_merge(&sheet_name, &range) {
            Ok(()) => Ok(Value::Bool(true).ref_cell()),
            Err(e) => Ok(nxlsx_err(span, e.message())),
        }
    })
}

// >>> nxlsx.freeze(handle, "Sheet1", 2, 1)
fn nxlsx_freeze(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nxlsx_freeze", span)?;
    let id = handle_arg(args, 0, "nxlsx_freeze", span)?;
    let row = int_arg(args, 2, "nxlsx_freeze", span)? as u32;
    let col = if args.len() >= 4 {
        int_arg(args, 3, "nxlsx_freeze", span)? as u32
    } else {
        0
    };
    BOOKS.with(|store| {
        let wb = match store.borrow_mut().get_mut(id) {
            Ok(w) => w,
            Err(e) => return Ok(invalid_handle(span, e.message())),
        };
        let sheet_name = sheet_name_arg(args, 1, wb, span)?;
        match wb.sheet_mut(&sheet_name) {
            Ok(sheet) => {
                sheet.freeze_row = Some(row);
                sheet.freeze_col = Some(col);
                wb.dirty = true;
                Ok(Value::Bool(true).ref_cell())
            }
            Err(e) => Ok(nxlsx_err(span, e.message())),
        }
    })
}

// >>> nxlsx.set_width(handle, "Sheet1", 1, 20.0)
fn nxlsx_set_width(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 4, 4, "nxlsx_set_width", span)?;
    let id = handle_arg(args, 0, "nxlsx_set_width", span)?;
    let col = int_arg(args, 2, "nxlsx_set_width", span)? as u32;
    let width = match &*args[3].borrow() {
        Value::Float(f) => *f,
        Value::Int(n) => *n as f64,
        other => {
            return Err(type_err(
                span,
                format!("width must be numeric, got {}", other.type_name()),
            ));
        }
    };
    BOOKS.with(|store| {
        let wb = match store.borrow_mut().get_mut(id) {
            Ok(w) => w,
            Err(e) => return Ok(invalid_handle(span, e.message())),
        };
        let sheet_name = sheet_name_arg(args, 1, wb, span)?;
        match wb.sheet_mut(&sheet_name) {
            Ok(sheet) => {
                sheet.col_widths.insert(col, width);
                wb.dirty = true;
                Ok(Value::Bool(true).ref_cell())
            }
            Err(e) => Ok(nxlsx_err(span, e.message())),
        }
    })
}

// >>> nxlsx.rows(handle, "Sheet1")
fn nxlsx_rows(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nxlsx_rows", span)?;
    let id = handle_arg(args, 0, "nxlsx_rows", span)?;
    BOOKS.with(|store| {
        let wb = match store.borrow().get(id) {
            Ok(w) => w,
            Err(e) => return Ok(invalid_handle(span, e.message())),
        };
        let name = sheet_name_arg(args, 1, wb, span)?;
        match wb.sheet(&name) {
            Ok(s) => Ok(Value::Int(s.nrows() as i64).ref_cell()),
            Err(e) => Ok(nxlsx_err(span, e.message())),
        }
    })
}

// >>> nxlsx.cols(handle, "Sheet1")
fn nxlsx_cols(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nxlsx_cols", span)?;
    let id = handle_arg(args, 0, "nxlsx_cols", span)?;
    BOOKS.with(|store| {
        let wb = match store.borrow().get(id) {
            Ok(w) => w,
            Err(e) => return Ok(invalid_handle(span, e.message())),
        };
        let name = sheet_name_arg(args, 1, wb, span)?;
        match wb.sheet(&name) {
            Ok(s) => Ok(Value::Int(s.ncols() as i64).ref_cell()),
            Err(e) => Ok(nxlsx_err(span, e.message())),
        }
    })
}

// >>> nxlsx.stream_open("big.xlsx", "Data", ["id", "name"])
fn nxlsx_stream_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nxlsx_stream_open", span)?;
    let path = string_arg(args, 0, "nxlsx_stream_open", span)?;
    let sheet = string_arg(args, 1, "nxlsx_stream_open", span)?;
    let headers = if args.len() >= 3 {
        match &*args[2].borrow() {
            Value::Array(items) => Some(
                items
                    .iter()
                    .map(|v| match &*v.borrow() {
                        Value::String(s) => Ok(s.clone()),
                        other => Err(type_err(
                            span,
                            format!("header must be string[], got {}", other.type_name()),
                        )),
                    })
                    .collect::<NiaoResult<Vec<_>>>()?,
            )
        } else {
            None
        }
    } else {
        None
    };
    let opts = write_opts_from_map(optional_object_arg(args, 3).as_ref());
    STREAMS.with(|store| match store.borrow_mut().open(Path::new(&path), &sheet, headers, &opts) {
        Ok(id) => Ok(Value::Int(id as i64).ref_cell()),
        Err(e) => Ok(map_xlsx_err(span, e)),
    })
}

// >>> nxlsx.stream_row(stream_id, [1, "a"])
fn nxlsx_stream_row(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nxlsx_stream_row", span)?;
    let id = handle_arg(args, 0, "nxlsx_stream_row", span)?;
    let cells = match cell_values_from_array(&*args[1].borrow()) {
        Ok(c) => c,
        Err(msg) => return Ok(nxlsx_err(span, msg)),
    };
    STREAMS.with(|store| match store.borrow_mut().write_row(id, &cells) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_xlsx_err(span, e)),
    })
}

// >>> nxlsx.stream_close(stream_id)
fn nxlsx_stream_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nxlsx_stream_close", span)?;
    let id = handle_arg(args, 0, "nxlsx_stream_close", span)?;
    STREAMS.with(|store| match store.borrow_mut().close(id) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_xlsx_err(span, e)),
    })
}

// >>> nxlsx.to_nframe(handle, "Data")
fn nxlsx_to_nframe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nxlsx_to_nframe", span)?;
    let id = handle_arg(args, 0, "nxlsx_to_nframe", span)?;
    let opts = read_opts_from_map(optional_object_arg(args, 2).as_ref());
    BOOKS.with(|store| {
        let wb = match store.borrow().get(id) {
            Ok(w) => w,
            Err(e) => return Ok(invalid_handle(span, e.message())),
        };
        let name = sheet_name_arg(args, 1, wb, span)?;
        let sheet = match wb.sheet(&name) {
            Ok(s) => s,
            Err(e) => return Ok(nxlsx_err(span, e.message())),
        };
        match sheet_to_table(sheet, opts.header, opts.infer_types) {
            Ok(table) => match table_to_dataframe(&table) {
                Ok(df) => Ok(Value::Int(super::nframe::store_frame(df) as i64).ref_cell()),
                Err(e) => Ok(nxlsx_err(span, e.message())),
            },
            Err(e) => Ok(nxlsx_err(span, e.message())),
        }
    })
}

// >>> nxlsx.from_nframe(handle, "Data", frame_id)
fn nxlsx_from_nframe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 3, "nxlsx_from_nframe", span)?;
    let id = handle_arg(args, 0, "nxlsx_from_nframe", span)?;
    let frame_id = match &*args[2].borrow() {
        Value::Int(n) if *n > 0 => *n as u64,
        other => {
            return Err(type_err(
                span,
                format!("from_nframe() expects frame handle, got {}", other.type_name()),
            ));
        }
    };
    let name = string_arg(args, 1, "nxlsx_from_nframe", span)?;
    match super::nframe::clone_frame(frame_id) {
        Some(df) => match dataframe_to_table(&df) {
            Ok(table) => BOOKS.with(|store| {
                match store.borrow_mut().get_mut(id) {
                    Ok(wb) => match write_table_to_sheet(wb, &name, &table, true) {
                        Ok(()) => Ok(Value::Bool(true).ref_cell()),
                        Err(e) => Ok(nxlsx_err(span, e.message())),
                    },
                    Err(e) => Ok(invalid_handle(span, e.message())),
                }
            }),
            Err(e) => Ok(nxlsx_err(span, e.message())),
        },
        None => Ok(nxlsx_err(span, "invalid frame handle")),
    }
}

// >>> nxlsx.table_rows(table)
fn nxlsx_table_rows(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nxlsx_table_rows", span)?;
    let map = object_arg(args, 0, "nxlsx_table_rows", span)?;
    match table_from_object(&map) {
        Ok(t) => Ok(Value::Int(t.nrows as i64).ref_cell()),
        Err(msg) => Ok(nxlsx_err(span, msg)),
    }
}

// >>> len(nxlsx.table_columns(table))
fn nxlsx_table_columns(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nxlsx_table_columns", span)?;
    let map = object_arg(args, 0, "nxlsx_table_columns", span)?;
    match table_from_object(&map) {
        Ok(t) => {
            let names: Vec<ValueRef> = t
                .column_names()
                .into_iter()
                .map(|n| Value::String(n).ref_cell())
                .collect();
            Ok(Value::Array(names).ref_cell())
        }
        Err(msg) => Ok(nxlsx_err(span, msg)),
    }
}

// >>> nxlsx.info("book.xlsx")
fn nxlsx_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nxlsx_info", span)?;
    let path = string_arg(args, 0, "nxlsx_info", span)?;
    match info_file(Path::new(&path)) {
        Ok(info) => {
            let mut map = HashMap::new();
            map.insert("path".to_string(), Value::String(info.path).ref_cell());
            map.insert(
                "file_size".to_string(),
                Value::Int(info.file_size as i64).ref_cell(),
            );
            let sheet_names: Vec<ValueRef> = info
                .sheet_names
                .into_iter()
                .map(|n| Value::String(n).ref_cell())
                .collect();
            map.insert("sheet_names".to_string(), Value::Array(sheet_names).ref_cell());
            let sheets: Vec<ValueRef> = info
                .sheets
                .into_iter()
                .map(|s| {
                    let mut m = HashMap::new();
                    m.insert("name".to_string(), Value::String(s.name).ref_cell());
                    if let Some(r) = s.rows {
                        m.insert("rows".to_string(), Value::Int(r as i64).ref_cell());
                    }
                    if let Some(c) = s.cols {
                        m.insert("cols".to_string(), Value::Int(c as i64).ref_cell());
                    }
                    Value::Object(m).ref_cell()
                })
                .collect();
            map.insert("sheets".to_string(), Value::Array(sheets).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_xlsx_err(span, e)),
    }
}

// >>> nxlsx.validate("book.xlsx")
fn nxlsx_validate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nxlsx_validate", span)?;
    match &*args[0].borrow() {
        Value::String(path) => Ok(Value::Bool(validate_file(Path::new(path))).ref_cell()),
        Value::ByteArray(bytes) => Ok(Value::Bool(validate_bytes(bytes)).ref_cell()),
        other => Err(type_err(
            span,
            format!("validate() expects path or byte[], got {}", other.type_name()),
        )),
    }
}

// >>> nxlsx.column_letter(1)
fn nxlsx_column_letter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nxlsx_column_letter", span)?;
    let col = int_arg(args, 0, "nxlsx_column_letter", span)? as u32;
    match column_letter(col) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(nxlsx_err(span, e.message())),
    }
}

// >>> nxlsx.column_index("AB")
fn nxlsx_column_index(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nxlsx_column_index", span)?;
    let letters = string_arg(args, 0, "nxlsx_column_index", span)?;
    match column_index(&letters) {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(e) => Ok(nxlsx_err(span, e.message())),
    }
}

// >>> nxlsx.load("book.xlsx")
fn nxlsx_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nxlsx_read(args, span)
}

// >>> nxlsx.load_workbook("book.xlsx")
fn nxlsx_load_workbook(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nxlsx_open(args, span)
}

macro_rules! nxlsx_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nxlsx_fns![
    ("nxlsx_create", "create", nxlsx_create),
    ("nxlsx_open", "open", nxlsx_open),
    ("nxlsx_close", "close", nxlsx_close),
    ("nxlsx_save", "save", nxlsx_save),
    ("nxlsx_to_bytes", "to_bytes", nxlsx_to_bytes),
    ("nxlsx_sheet_names", "sheet_names", nxlsx_sheet_names),
    ("nxlsx_active_sheet", "active_sheet", nxlsx_active_sheet),
    ("nxlsx_set_active", "set_active", nxlsx_set_active),
    ("nxlsx_add_sheet", "add_sheet", nxlsx_add_sheet),
    ("nxlsx_remove_sheet", "remove_sheet", nxlsx_remove_sheet),
    ("nxlsx_rename_sheet", "rename_sheet", nxlsx_rename_sheet),
    ("nxlsx_read", "read", nxlsx_read),
    ("nxlsx_read_sheet", "read_sheet", nxlsx_read_sheet),
    ("nxlsx_read_rows", "read_rows", nxlsx_read_rows),
    ("nxlsx_read_chunk", "read_chunk", nxlsx_read_chunk),
    ("nxlsx_write", "write", nxlsx_write),
    ("nxlsx_write_sheet", "write_sheet", nxlsx_write_sheet),
    ("nxlsx_set_cell", "set_cell", nxlsx_set_cell),
    ("nxlsx_cell", "cell", nxlsx_cell),
    ("nxlsx_formula", "formula", nxlsx_formula),
    ("nxlsx_style", "style", nxlsx_style),
    ("nxlsx_merge", "merge", nxlsx_merge),
    ("nxlsx_freeze", "freeze", nxlsx_freeze),
    ("nxlsx_set_width", "set_width", nxlsx_set_width),
    ("nxlsx_rows", "rows", nxlsx_rows),
    ("nxlsx_cols", "cols", nxlsx_cols),
    ("nxlsx_stream_open", "stream_open", nxlsx_stream_open),
    ("nxlsx_stream_row", "stream_row", nxlsx_stream_row),
    ("nxlsx_stream_close", "stream_close", nxlsx_stream_close),
    ("nxlsx_to_nframe", "to_nframe", nxlsx_to_nframe),
    ("nxlsx_from_nframe", "from_nframe", nxlsx_from_nframe),
    ("nxlsx_table_rows", "table_rows", nxlsx_table_rows),
    ("nxlsx_table_columns", "table_columns", nxlsx_table_columns),
    ("nxlsx_info", "info", nxlsx_info),
    ("nxlsx_validate", "validate", nxlsx_validate),
    ("nxlsx_column_letter", "column_letter", nxlsx_column_letter),
    ("nxlsx_column_index", "column_index", nxlsx_column_index),
    ("nxlsx_load", "load", nxlsx_load),
    ("nxlsx_load_workbook", "load_workbook", nxlsx_load_workbook),
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

pub const MODULE_NAME: &str = "nxlsx";
pub const MODULE_PATHS: &[&str] = &["nxlsx", "std/nxlsx"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}
