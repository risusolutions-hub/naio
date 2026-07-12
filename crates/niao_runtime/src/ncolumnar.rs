//! Native ncolumnar standard library — column-major binary codec for tables.
//! Wire format magic `NCOL1`.
//!
//! Import with `import "ncolumnar"` (or `import "std/ncolumnar"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::rc::Rc;

const E3430_NCOLUMNAR_ARITY: u32 = 3430;
const E3431_NCOLUMNAR_ERROR: u32 = 3431;
const E3432_NCOLUMNAR_TYPE: u32 = 3432;
const E3433_NCOLUMNAR_FORMAT: u32 = 3433;

const MAGIC: &[u8; 5] = b"NCOL1";
const VERSION: u8 = 1;

const COL_INT: u8 = 0;
const COL_FLOAT: u8 = 1;
const COL_BOOL: u8 = 2;
const COL_STRING: u8 = 3;
const COL_NIL: u8 = 4;

// ---------------------------------------------------------------------------
// Column model
// ---------------------------------------------------------------------------

enum Column {
    Int(Vec<i64>),
    Float(Vec<f64>),
    Bool(Vec<u8>),
    String(Vec<String>),
    Nil(usize),
}

impl Column {
    fn len(&self) -> usize {
        match self {
            Column::Int(v) => v.len(),
            Column::Float(v) => v.len(),
            Column::Bool(v) => v.len(),
            Column::String(v) => v.len(),
            Column::Nil(n) => *n,
        }
    }

    fn tag(&self) -> u8 {
        match self {
            Column::Int(_) => COL_INT,
            Column::Float(_) => COL_FLOAT,
            Column::Bool(_) => COL_BOOL,
            Column::String(_) => COL_STRING,
            Column::Nil(_) => COL_NIL,
        }
    }

    fn to_value(&self) -> Value {
        match self {
            Column::Int(v) => Value::IntArray(v.clone()),
            Column::Float(v) => Value::FloatArray(v.clone()),
            Column::Bool(v) => Value::BoolArray(v.clone()),
            Column::String(v) => Value::StringArray(crate::StringArray::dense(v.clone())),
            Column::Nil(n) => {
                let items: Vec<ValueRef> = (0..*n).map(|_| Value::Nil.ref_cell()).collect();
                Value::Array(items)
            }
        }
    }
}

struct Table {
    rows: usize,
    columns: Vec<(String, Column)>,
}

// ---------------------------------------------------------------------------
// Binary codec
// ---------------------------------------------------------------------------

struct Encoder {
    buf: Vec<u8>,
}

impl Encoder {
    fn new() -> Self {
        Encoder { buf: Vec::new() }
    }

    fn push_u16(&mut self, n: u16) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    fn push_u32(&mut self, n: u32) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    fn push_i64(&mut self, n: i64) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    fn push_f64(&mut self, n: f64) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    fn push_bytes(&mut self, b: &[u8]) {
        self.push_u32(b.len() as u32);
        self.buf.extend_from_slice(b);
    }

    fn encode_column(&mut self, col: &Column) {
        self.buf.push(col.tag());
        match col {
            Column::Int(v) => {
                self.push_u32(v.len() as u32);
                for n in v {
                    self.push_i64(*n);
                }
            }
            Column::Float(v) => {
                self.push_u32(v.len() as u32);
                for f in v {
                    self.push_f64(*f);
                }
            }
            Column::Bool(v) => {
                self.push_u32(v.len() as u32);
                self.buf.extend_from_slice(v);
            }
            Column::String(v) => {
                self.push_u32(v.len() as u32);
                let mut blob = Vec::new();
                let mut offsets = Vec::with_capacity(v.len() + 1);
                offsets.push(0u32);
                for s in v {
                    blob.extend_from_slice(s.as_bytes());
                    offsets.push(blob.len() as u32);
                }
                self.push_u32(offsets.len() as u32);
                for off in offsets {
                    self.push_u32(off);
                }
                self.push_bytes(&blob);
            }
            Column::Nil(n) => {
                self.push_u32(*n as u32);
            }
        }
    }

    fn finish(self) -> Vec<u8> {
        self.buf
    }
}

struct Decoder<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    fn new(data: &'a [u8]) -> Self {
        Decoder { data, pos: 0 }
    }

    fn read_exact(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.pos + n > self.data.len() {
            return Err("unexpected end of columnar data".into());
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn read_u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.read_exact(2)?.try_into().unwrap()))
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.read_exact(4)?.try_into().unwrap()))
    }

    fn read_i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_le_bytes(self.read_exact(8)?.try_into().unwrap()))
    }

    fn read_f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_le_bytes(self.read_exact(8)?.try_into().unwrap()))
    }

    fn read_bytes(&mut self) -> Result<Vec<u8>, String> {
        let len = self.read_u32()? as usize;
        Ok(self.read_exact(len)?.to_vec())
    }

    fn read_string(&mut self) -> Result<String, String> {
        let bytes = self.read_bytes()?;
        String::from_utf8(bytes).map_err(|e| format!("invalid UTF-8 in column name: {e}"))
    }

    fn decode_column(&mut self, expected_rows: usize) -> Result<Column, String> {
        let tag = self.read_exact(1)?[0];
        match tag {
            COL_INT => {
                let len = self.read_u32()? as usize;
                if len != expected_rows {
                    return Err(format!("int column row count {len} != table rows {expected_rows}"));
                }
                let mut v = Vec::with_capacity(len);
                for _ in 0..len {
                    v.push(self.read_i64()?);
                }
                Ok(Column::Int(v))
            }
            COL_FLOAT => {
                let len = self.read_u32()? as usize;
                if len != expected_rows {
                    return Err(format!(
                        "float column row count {len} != table rows {expected_rows}"
                    ));
                }
                let mut v = Vec::with_capacity(len);
                for _ in 0..len {
                    v.push(self.read_f64()?);
                }
                Ok(Column::Float(v))
            }
            COL_BOOL => {
                let len = self.read_u32()? as usize;
                if len != expected_rows {
                    return Err(format!("bool column row count {len} != table rows {expected_rows}"));
                }
                Ok(Column::Bool(self.read_exact(len)?.to_vec()))
            }
            COL_STRING => {
                let len = self.read_u32()? as usize;
                if len != expected_rows {
                    return Err(format!(
                        "string column row count {len} != table rows {expected_rows}"
                    ));
                }
                let off_count = self.read_u32()? as usize;
                if off_count != len + 1 {
                    return Err("string column offset table size mismatch".into());
                }
                let mut offsets = Vec::with_capacity(off_count);
                for _ in 0..off_count {
                    offsets.push(self.read_u32()?);
                }
                let blob = self.read_bytes()?;
                let mut items = Vec::with_capacity(len);
                for i in 0..len {
                    let start = offsets[i] as usize;
                    let end = offsets[i + 1] as usize;
                    if end > blob.len() || start > end {
                        return Err("string column offset out of range".into());
                    }
                    let s = std::str::from_utf8(&blob[start..end])
                        .map_err(|e| format!("invalid UTF-8 in string column: {e}"))?;
                    items.push(s.to_string());
                }
                Ok(Column::String(items))
            }
            COL_NIL => {
                let len = self.read_u32()? as usize;
                if len != expected_rows {
                    return Err(format!("nil column row count {len} != table rows {expected_rows}"));
                }
                Ok(Column::Nil(len))
            }
            _ => Err(format!("unknown column type tag 0x{tag:02x}")),
        }
    }
}

fn column_from_value(name: &str, v: &Value, rows: usize) -> Result<Column, String> {
    match v {
        Value::IntArray(items) => {
            if items.len() != rows {
                return Err(format!(
                    "column '{name}' length {} != table rows {rows}",
                    items.len()
                ));
            }
            Ok(Column::Int(items.clone()))
        }
        Value::FloatArray(items) => {
            if items.len() != rows {
                return Err(format!(
                    "column '{name}' length {} != table rows {rows}",
                    items.len()
                ));
            }
            Ok(Column::Float(items.clone()))
        }
        Value::BoolArray(items) => {
            if items.len() != rows {
                return Err(format!(
                    "column '{name}' length {} != table rows {rows}",
                    items.len()
                ));
            }
            Ok(Column::Bool(items.clone()))
        }
        Value::StringArray(sa) => {
            let items = sa.dense_vec();
            if items.len() != rows {
                return Err(format!(
                    "column '{name}' length {} != table rows {rows}",
                    items.len()
                ));
            }
            Ok(Column::String(items))
        }
        Value::Array(items) => {
            if items.len() != rows {
                return Err(format!(
                    "column '{name}' length {} != table rows {rows}",
                    items.len()
                ));
            }
            if items.iter().all(|c| matches!(&*c.borrow(), Value::Nil)) {
                return Ok(Column::Nil(rows));
            }
            Err(format!(
                "column '{name}' array must be all nil or use a typed column (int[], float[], bool[], string[])"
            ))
        }
        other => Err(format!(
            "column '{name}' must be a typed array, got {}",
            other.type_name()
        )),
    }
}

fn table_from_object(map: &HashMap<String, ValueRef>) -> Result<Table, String> {
    if map.is_empty() {
        return Err("table must have at least one column".into());
    }
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    let first = keys[0];
    let rows = match &*map[first].borrow() {
        Value::IntArray(v) => v.len(),
        Value::FloatArray(v) => v.len(),
        Value::BoolArray(v) => v.len(),
        Value::StringArray(sa) => sa.len(),
        Value::Array(v) => v.len(),
        other => {
            return Err(format!(
                "column '{first}' must be an array, got {}",
                other.type_name()
            ));
        }
    };

    let mut columns = Vec::with_capacity(keys.len());
    for k in keys {
        let col = column_from_value(k, &map[k].borrow(), rows)?;
        columns.push(((*k).clone(), col));
    }
    Ok(Table { rows, columns })
}

fn encode_table(table: &Table) -> Vec<u8> {
    let mut enc = Encoder::new();
    enc.buf.extend_from_slice(MAGIC);
    enc.buf.push(VERSION);
    enc.push_u32(table.rows as u32);
    enc.push_u16(table.columns.len() as u16);
    for (name, col) in &table.columns {
        enc.push_u16(name.len() as u16);
        enc.buf.extend_from_slice(name.as_bytes());
        enc.encode_column(col);
    }
    enc.finish()
}

fn decode_table(bytes: &[u8]) -> Result<Table, String> {
    if bytes.len() < 12 {
        return Err("columnar data too short".into());
    }
    if &bytes[..5] != MAGIC {
        return Err("invalid columnar magic (expected NCOL1)".into());
    }
    if bytes[5] != VERSION {
        return Err(format!("unsupported columnar version {}", bytes[5]));
    }
    let rows = u32::from_le_bytes(bytes[6..10].try_into().unwrap()) as usize;
    let num_cols = u16::from_le_bytes(bytes[10..12].try_into().unwrap()) as usize;
    let mut dec = Decoder::new(&bytes[12..]);
    let mut columns = Vec::with_capacity(num_cols);
    for _ in 0..num_cols {
        let name_len = dec.read_u16()? as usize;
        let name_bytes = dec.read_exact(name_len)?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|e| format!("invalid UTF-8 column name: {e}"))?
            .to_string();
        let col = dec.decode_column(rows)?;
        columns.push((name, col));
    }
    if dec.pos != dec.data.len() {
        return Err("trailing bytes in columnar payload".into());
    }
    Ok(Table { rows, columns })
}

fn table_to_object(table: &Table) -> Value {
    let mut map = HashMap::new();
    for (name, col) in &table.columns {
        map.insert(name.clone(), col.to_value().ref_cell());
    }
    Value::Object(map)
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3432_NCOLUMNAR_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3430_NCOLUMNAR_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
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

fn ncolumnar_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3431_NCOLUMNAR_ERROR, "ncolumnar_error", msg.into(), span)
}

fn ncolumnar_format_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3433_NCOLUMNAR_FORMAT, "ncolumnar_error", msg.into(), span)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ncolumnar_encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncolumnar_encode", span)?;
    let map = object_arg(args, 0, "ncolumnar_encode", span)?;
    match table_from_object(&map) {
        Ok(table) => Ok(Value::ByteArray(encode_table(&table)).ref_cell()),
        Err(msg) => Ok(ncolumnar_err(span, msg)),
    }
}

fn ncolumnar_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncolumnar_decode", span)?;
    let bytes = bytes_arg(args, 0, "ncolumnar_decode", span)?;
    match decode_table(&bytes) {
        Ok(table) => Ok(table_to_object(&table).ref_cell()),
        Err(msg) => Ok(ncolumnar_format_err(span, msg)),
    }
}

fn ncolumnar_validate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncolumnar_validate", span)?;
    let bytes = bytes_arg(args, 0, "ncolumnar_validate", span)?;
    Ok(Value::Bool(decode_table(&bytes).is_ok()).ref_cell())
}

fn ncolumnar_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncolumnar_info", span)?;
    let bytes = bytes_arg(args, 0, "ncolumnar_info", span)?;
    match decode_table(&bytes) {
        Ok(table) => {
            let mut map = HashMap::new();
            map.insert("magic".to_string(), Value::String("NCOL1".into()).ref_cell());
            map.insert("version".to_string(), Value::Int(VERSION as i64).ref_cell());
            map.insert("rows".to_string(), Value::Int(table.rows as i64).ref_cell());
            map.insert(
                "cols".to_string(),
                Value::Int(table.columns.len() as i64).ref_cell(),
            );
            let names: Vec<ValueRef> = table
                .columns
                .iter()
                .map(|(n, _)| Value::String(n.clone()).ref_cell())
                .collect();
            map.insert("columns".to_string(), Value::Array(names).ref_cell());
            let types: Vec<ValueRef> = table
                .columns
                .iter()
                .map(|(_, c)| {
                    let label = match c {
                        Column::Int(_) => "int",
                        Column::Float(_) => "float",
                        Column::Bool(_) => "bool",
                        Column::String(_) => "string",
                        Column::Nil(_) => "nil",
                    };
                    Value::String(label.into()).ref_cell()
                })
                .collect();
            map.insert("types".to_string(), Value::Array(types).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(msg) => Ok(ncolumnar_format_err(span, msg)),
    }
}

fn ncolumnar_rows(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncolumnar_rows", span)?;
    let map = object_arg(args, 0, "ncolumnar_rows", span)?;
    match table_from_object(&map) {
        Ok(table) => Ok(Value::Int(table.rows as i64).ref_cell()),
        Err(msg) => Ok(ncolumnar_err(span, msg)),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncolumnar_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncolumnar_fns![
    ("ncolumnar_encode", "encode", ncolumnar_encode),
    ("ncolumnar_decode", "decode", ncolumnar_decode),
    ("ncolumnar_validate", "validate", ncolumnar_validate),
    ("ncolumnar_info", "info", ncolumnar_info),
    ("ncolumnar_rows", "rows", ncolumnar_rows),
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

pub const MODULE_NAME: &str = "ncolumnar";
pub const MODULE_PATHS: &[&str] = &["ncolumnar", "std/ncolumnar"];

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
            Value::StringArray(crate::StringArray::dense(vec![
                "a".into(),
                "b".into(),
                "c".into(),
            ]))
            .ref_cell(),
        );
        Value::Object(map).ref_cell()
    }

    #[test]
    fn roundtrip_encode_decode() {
        let table = sample_table();
        let bytes = ncolumnar_encode(&[table.clone()], span()).unwrap();
        let restored = ncolumnar_decode(&[bytes], span()).unwrap();
        assert!(crate::values_equal(&table.borrow(), &restored.borrow()));
    }

    #[test]
    fn info_reports_shape() {
        let table = sample_table();
        let bytes = ncolumnar_encode(&[table], span()).unwrap();
        let info = ncolumnar_info(&[bytes], span()).unwrap();
        let ir = info.borrow();
        match &*ir {
            Value::Object(map) => {
                assert!(matches!(&*map.get("rows").unwrap().borrow(), Value::Int(3)));
                assert!(matches!(&*map.get("cols").unwrap().borrow(), Value::Int(3)));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_bad_magic() {
        let bad = Value::ByteArray(b"NOPE!".to_vec()).ref_cell();
        let ok = ncolumnar_validate(&[bad], span()).unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(false)));
    }
}
