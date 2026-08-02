//! Native ncsv standard library — lightweight RFC4180-ish CSV parse/stringify
//! and file read/write. Returns row arrays or header-keyed objects.
//!
//! Import with `import "ncsv"` (or `import "std/ncsv"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::rc::Rc;

// Wired into niao_errors::codes by central integration.
const E2850_NCSV_ARITY: u32 = 2850;
const E2851_NCSV_ERROR: u32 = 2851;
const E2853_NCSV_PARSE: u32 = 2853;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::TypeError {
        message: msg.into(),
        line: span.line,
        col: span.col,
    }
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
            E2850_NCSV_ARITY,
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

fn string_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: &str) -> String {
    let Some(map) = map else {
        return default.to_string();
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) if !s.is_empty() => s,
        _ => default.to_string(),
    }
}

fn char_field(
    map: Option<&HashMap<String, ValueRef>>,
    key: &str,
    default: char,
    span: Span,
) -> Result<char, RuntimeError> {
    let s = string_field(map, key, &default.to_string());
    let mut chars = s.chars();
    let first = chars
        .next()
        .ok_or_else(|| type_err(span, format!("opts.{key} must be a non-empty string")))?;
    if chars.next().is_some() {
        return Err(type_err(
            span,
            format!("opts.{key} must be a single character, got '{s}'"),
        ));
    }
    Ok(first)
}

fn names_field(
    map: Option<&HashMap<String, ValueRef>>,
    span: Span,
) -> Result<Option<Vec<String>>, RuntimeError> {
    let Some(map) = map else {
        return Ok(None);
    };
    let Some(val) = map.get("names") else {
        return Ok(None);
    };
    match &*val.borrow() {
        Value::Array(items) => {
            let mut names = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => names.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "opts.names[{}] must be a string, got {}",
                                i,
                                other.type_name()
                            ),
                        ))
                    }
                }
            }
            Ok(Some(names))
        }
        other => Err(type_err(
            span,
            format!(
                "opts.names must be an array of strings, got {}",
                other.type_name()
            ),
        )),
    }
}

fn ncsv_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2851_NCSV_ERROR, "ncsv_error", msg.into(), span)
}

fn ncsv_parse_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2853_NCSV_PARSE, "ncsv_error", msg.into(), span)
}

fn str_val(s: String) -> NiaoResult<ValueRef> {
    Ok(Value::String(s).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

// ---------------------------------------------------------------------------
// CSV options
// ---------------------------------------------------------------------------

struct CsvOpts {
    header: bool,
    delimiter: char,
    quote: char,
    names: Option<Vec<String>>,
}

fn parse_opts(args: &[ValueRef], idx: usize, span: Span) -> Result<CsvOpts, RuntimeError> {
    let map = optional_object_arg(args, idx);
    Ok(CsvOpts {
        header: bool_field(map.as_ref(), "header", false),
        delimiter: char_field(map.as_ref(), "delimiter", ',', span)?,
        quote: char_field(map.as_ref(), "quote", '"', span)?,
        names: names_field(map.as_ref(), span)?,
    })
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

fn parse_records(text: &str, delimiter: char, quote: char) -> Result<Vec<Vec<String>>, String> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == quote {
                if chars.peek() == Some(&quote) {
                    chars.next();
                    field.push(quote);
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }

        match c {
            q if q == quote => in_quotes = true,
            d if d == delimiter => {
                record.push(std::mem::take(&mut field));
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            other => field.push(other),
        }
    }

    if in_quotes {
        return Err("unclosed quoted field".to_string());
    }

    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    } else if text.ends_with('\n') || text.ends_with("\r\n") {
        records.push(Vec::new());
    }

    // Drop a single trailing empty record from a final newline.
    if records.len() > 1 && records.last().map(|r| r.is_empty()).unwrap_or(false) {
        records.pop();
    }

    Ok(records)
}

fn value_to_cell(v: &Value) -> String {
    match v {
        Value::Nil => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn records_to_rows(records: Vec<Vec<String>>) -> ValueRef {
    Value::Array(
        records
            .into_iter()
            .map(|row| {
                Value::Array(
                    row.into_iter()
                        .map(|cell| Value::String(cell).ref_cell())
                        .collect(),
                )
                .ref_cell()
            })
            .collect(),
    )
    .ref_cell()
}

fn records_to_objects(
    records: Vec<Vec<String>>,
    header_names: Vec<String>,
    data_start: usize,
) -> ValueRef {
    let rows: Vec<ValueRef> = records[data_start..]
        .iter()
        .map(|row| {
            let mut obj = HashMap::new();
            for (i, name) in header_names.iter().enumerate() {
                let cell = row.get(i).cloned().unwrap_or_default();
                obj.insert(name.clone(), Value::String(cell).ref_cell());
            }
            Value::Object(obj).ref_cell()
        })
        .collect();
    Value::Array(rows).ref_cell()
}

fn parse_csv_text(text: &str, opts: &CsvOpts, span: Span) -> Result<ValueRef, ValueRef> {
    let records = match parse_records(text, opts.delimiter, opts.quote) {
        Ok(r) => r,
        Err(msg) => return Err(ncsv_parse_error(span, msg)),
    };

    if !opts.header {
        return Ok(records_to_rows(records));
    }

    let header_names = if let Some(names) = &opts.names {
        names.clone()
    } else if let Some(first) = records.first() {
        first.clone()
    } else {
        return Ok(Value::Array(Vec::new()).ref_cell());
    };

    let data_start = if opts.names.is_some() { 0 } else { 1 };
    Ok(records_to_objects(records, header_names, data_start))
}

fn escape_field(cell: &str, delimiter: char, quote: char) -> String {
    let needs_quote = cell.contains(delimiter)
        || cell.contains(quote)
        || cell.contains('\n')
        || cell.contains('\r')
        || cell.starts_with(' ')
        || cell.ends_with(' ');
    if !needs_quote {
        return cell.to_string();
    }
    let mut escaped = String::with_capacity(cell.len() + 2);
    for c in cell.chars() {
        if c == quote {
            escaped.push(quote);
        }
        escaped.push(c);
    }
    format!("{quote}{escaped}{quote}")
}

fn object_columns(map: &HashMap<String, ValueRef>) -> Vec<String> {
    let mut names: Vec<String> = map.keys().cloned().collect();
    names.sort();
    names
}

fn object_row(map: &HashMap<String, ValueRef>, columns: &[String]) -> Vec<String> {
    columns
        .iter()
        .map(|k| {
            map.get(k)
                .map(|v| value_to_cell(&v.borrow()))
                .unwrap_or_default()
        })
        .collect()
}

fn stringify_records(records: &[Vec<String>], delimiter: char, quote: char) -> String {
    let mut out = String::new();
    for (ri, record) in records.iter().enumerate() {
        if ri > 0 {
            out.push('\n');
        }
        for (fi, cell) in record.iter().enumerate() {
            if fi > 0 {
                out.push(delimiter);
            }
            out.push_str(&escape_field(cell, delimiter, quote));
        }
    }
    out
}

fn rows_to_records(
    rows: &ValueRef,
    columns: Option<&[String]>,
    span: Span,
) -> Result<Vec<Vec<String>>, RuntimeError> {
    match &*rows.borrow() {
        Value::Array(items) => {
            let mut records = Vec::with_capacity(items.len());
            for (ri, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Array(cells) => {
                        records.push(cells.iter().map(|c| value_to_cell(&c.borrow())).collect());
                    }
                    Value::Object(map) => {
                        let cols = columns
                            .map(|c| c.to_vec())
                            .unwrap_or_else(|| object_columns(map));
                        records.push(object_row(map, &cols));
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "rows[{}] must be an array or object, got {}",
                                ri,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(records)
        }
        other => Err(type_err(
            span,
            format!("rows must be an array, got {}", other.type_name()),
        )),
    }
}

fn stringify_rows(rows: &ValueRef, opts: &CsvOpts, span: Span) -> Result<String, RuntimeError> {
    let columns = opts.names.clone().or_else(|| match &*rows.borrow() {
        Value::Array(items) => items.first().and_then(|item| match &*item.borrow() {
            Value::Object(map) => Some(object_columns(map)),
            _ => None,
        }),
        _ => None,
    });
    let col_slice = columns.as_deref();
    let mut records = rows_to_records(rows, col_slice, span)?;

    if opts.header {
        if let Some(header) = columns {
            if !header.is_empty() {
                records.insert(0, header);
            }
        }
    }

    Ok(stringify_records(&records, opts.delimiter, opts.quote))
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ncsv_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncsv_parse", span)?;
    let text = string_arg(args, 0, "ncsv_parse", span)?;
    let opts = parse_opts(args, 1, span)?;
    match parse_csv_text(&text, &opts, span) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn ncsv_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncsv_read", span)?;
    let path = string_arg(args, 0, "ncsv_read", span)?;
    let opts = parse_opts(args, 1, span)?;
    let text = match fs::read_to_string(Path::new(&path)) {
        Ok(t) => t,
        Err(e) => return Ok(ncsv_error(span, format!("read '{path}': {e}"))),
    };
    match parse_csv_text(&text, &opts, span) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn ncsv_stringify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncsv_stringify", span)?;
    let opts = parse_opts(args, 1, span)?;
    match stringify_rows(&args[0], &opts, span) {
        Ok(s) => str_val(s),
        Err(e) => Err(e),
    }
}

fn ncsv_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncsv_write", span)?;
    let path = string_arg(args, 0, "ncsv_write", span)?;
    let opts = parse_opts(args, 2, span)?;
    let csv = match stringify_rows(&args[1], &opts, span) {
        Ok(s) => s,
        Err(e) => return Err(e),
    };
    if let Err(e) = fs::write(Path::new(&path), csv) {
        return Ok(ncsv_error(span, format!("write '{path}': {e}")));
    }
    bool_val(true)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncsv_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncsv_fns![
    ("ncsv_parse", "parse", ncsv_parse),
    ("ncsv_read", "read", ncsv_read),
    ("ncsv_stringify", "stringify", ncsv_stringify),
    ("ncsv_write", "write", ncsv_write),
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

pub const MODULE_NAME: &str = "ncsv";
pub const MODULE_PATHS: &[&str] = &["ncsv", "std/ncsv"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    fn default_opts() -> CsvOpts {
        CsvOpts {
            header: false,
            delimiter: ',',
            quote: '"',
            names: None,
        }
    }

    #[test]
    fn quoted_commas() {
        let records = parse_records(r#""a,b",c"#, ',', '"').unwrap();
        assert_eq!(records, vec![vec!["a,b".to_string(), "c".to_string()]]);
    }

    #[test]
    fn escaped_quotes() {
        let records = parse_records(r#""say ""hi""",ok"#, ',', '"').unwrap();
        assert_eq!(
            records,
            vec![vec!["say \"hi\"".to_string(), "ok".to_string()]]
        );
    }

    #[test]
    fn header_from_first_row() {
        let opts = CsvOpts {
            header: true,
            delimiter: ',',
            quote: '"',
            names: None,
        };
        let text = "name,age\nalice,30\nbob,25";
        let rows = parse_csv_text(text, &opts, span()).unwrap();
        match &*rows.borrow() {
            Value::Array(items) => {
                assert_eq!(items.len(), 2);
                match &*items[0].borrow() {
                    Value::Object(map) => {
                        assert_eq!(value_to_cell(&map.get("name").unwrap().borrow()), "alice");
                        assert_eq!(value_to_cell(&map.get("age").unwrap().borrow()), "30");
                    }
                    other => panic!("expected object, got {other:?}"),
                }
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn header_with_names() {
        let opts = CsvOpts {
            header: true,
            delimiter: ',',
            quote: '"',
            names: Some(vec!["x".to_string(), "y".to_string()]),
        };
        let text = "1,2\n3,4";
        let rows = parse_csv_text(text, &opts, span()).unwrap();
        match &*rows.borrow() {
            Value::Array(items) => {
                assert_eq!(items.len(), 2);
                match &*items[0].borrow() {
                    Value::Object(map) => {
                        assert_eq!(value_to_cell(&map.get("x").unwrap().borrow()), "1");
                        assert_eq!(value_to_cell(&map.get("y").unwrap().borrow()), "2");
                    }
                    other => panic!("expected object, got {other:?}"),
                }
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn stringify_roundtrip() {
        let rows = records_to_rows(vec![
            vec!["plain".to_string(), "a,b".to_string()],
            vec!["x".to_string(), "y".to_string()],
        ]);
        let csv = stringify_rows(&rows, &default_opts(), span()).unwrap();
        assert_eq!(csv, "plain,\"a,b\"\nx,y");
        assert_eq!(csv, "plain,\"a,b\"\nx,y");
    }
}
