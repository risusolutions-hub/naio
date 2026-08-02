//! Native nbinary standard library — struct pack/unpack, endianness, bit fields,
//! varints, CRC32/64 (~Python struct + bitstring subset).
//!
//! Import with `import "nbinary"` (or `import "std/nbinary"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_binary::{
    crc32, crc32_update, crc64, crc64_update,     uvarint_decode, uvarint_encode, varint_decode,
    varint_encode, zigzag_decode, zigzag_encode, BitString, CompiledStruct, PackValue,
    UnpackValue,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3460_NBINARY_ARITY: u32 = 3460;
const E3461_NBINARY_ERROR: u32 = 3461;
const E3462_NBINARY_TYPE: u32 = 3462;
const E3463_NBINARY_INVALID_HANDLE: u32 = 3463;

thread_local! {
    static BITS: RefCell<HashMap<i64, BitString>> = RefCell::new(HashMap::new());
    static NEXT_BITS_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn new_bits_handle() -> i64 {
    NEXT_BITS_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3462_NBINARY_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3460_NBINARY_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3460_NBINARY_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn binary_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3461_NBINARY_ERROR, "nbinary_error", msg.into(), span)
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

fn optional_int(args: &[ValueRef], idx: usize, default: i64) -> i64 {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Int(n) => *n,
        _ => default,
    }
}

fn optional_bool(args: &[ValueRef], idx: usize, default: bool) -> bool {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        _ => default,
    }
}

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects bytes as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn bytes_val(b: Vec<u8>) -> NiaoResult<ValueRef> {
    Ok(Value::ByteArray(b).ref_cell())
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn float_val(f: f64) -> NiaoResult<ValueRef> {
    Ok(Value::Float(f).ref_cell())
}

fn array_val(items: Vec<ValueRef>) -> NiaoResult<ValueRef> {
    Ok(Value::Array(items).ref_cell())
}

fn compile_fmt(fmt: &str, span: Span) -> Result<CompiledStruct, ValueRef> {
    CompiledStruct::parse(fmt).map_err(|e| binary_err(span, e))
}

fn value_to_pack(v: &Value, span: Span) -> Result<PackValue, ValueRef> {
    match v {
        Value::Int(n) => Ok(PackValue::I64(*n)),
        Value::Float(f) => Ok(PackValue::F64(*f)),
        Value::Bool(b) => Ok(PackValue::Bool(*b)),
        Value::String(s) => Ok(PackValue::Bytes(s.as_bytes().to_vec())),
        Value::ByteArray(b) => Ok(PackValue::Bytes(b.clone())),
        other => Err(binary_err(
            span,
            format!("cannot pack value of type {}", other.type_name()),
        )),
    }
}

fn unpack_to_value(v: UnpackValue) -> ValueRef {
    match v {
        UnpackValue::I8(n) => Value::Int(n as i64).ref_cell(),
        UnpackValue::U8(n) => Value::Int(n as i64).ref_cell(),
        UnpackValue::I16(n) => Value::Int(n as i64).ref_cell(),
        UnpackValue::U16(n) => Value::Int(n as i64).ref_cell(),
        UnpackValue::I32(n) => Value::Int(n as i64).ref_cell(),
        UnpackValue::U32(n) => Value::Int(n as i64).ref_cell(),
        UnpackValue::I64(n) => Value::Int(n).ref_cell(),
        UnpackValue::U64(n) => Value::Int(n as i64).ref_cell(),
        UnpackValue::Isize(n) => Value::Int(n as i64).ref_cell(),
        UnpackValue::Usize(n) => Value::Int(n as i64).ref_cell(),
        UnpackValue::F32(n) => Value::Float(f64::from(n)).ref_cell(),
        UnpackValue::F64(n) => Value::Float(n).ref_cell(),
        UnpackValue::Bool(b) => Value::Bool(b).ref_cell(),
        UnpackValue::Bytes(b) => Value::ByteArray(b).ref_cell(),
        UnpackValue::Char(c) => Value::Int(c as i64).ref_cell(),
        UnpackValue::Pointer(p) => Value::Int(p as i64).ref_cell(),
    }
}

fn with_bits<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut BitString) -> Result<T, String>,
) -> NiaoResult<Result<T, ValueRef>> {
    BITS.with(|map| {
        let mut map = map.borrow_mut();
        match map.get_mut(&id) {
            Some(bs) => Ok(f(bs).map_err(|e| binary_err(span, e))),
            None => Ok(Err(error_value(
                E3463_NBINARY_INVALID_HANDLE,
                "nbinary_error",
                format!("invalid or released bits handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Struct pack/unpack
// ---------------------------------------------------------------------------

// >>> import "nbinary"
// >>> nbinary.pack(">I", 16909060)
// => byte_array[1, 2, 3, 4]
fn nbinary_pack(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() < 2 {
        return Err(RuntimeError::at(
            span,
            E3460_NBINARY_ARITY,
            format!("nbinary_pack() expects at least 2 argument(s), got {}", args.len()),
        ));
    }
    let fmt = string_arg(args, 0, "nbinary_pack", span)?;
    let compiled = match compile_fmt(&fmt, span) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    let mut values = Vec::new();
    for arg in &args[1..] {
        match value_to_pack(&arg.borrow(), span) {
            Ok(v) => values.push(v),
            Err(e) => return Ok(e),
        }
    }
    match compiled.pack(&values) {
        Ok(buf) => bytes_val(buf),
        Err(e) => Ok(binary_err(span, e)),
    }
}

// >>> nbinary.unpack(">I", byte_array[1, 2, 3, 4])
// => [16909060]
fn nbinary_unpack(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nbinary_unpack", span)?;
    let fmt = string_arg(args, 0, "nbinary_unpack", span)?;
    let data = bytes_arg(args, 1, "nbinary_unpack", span)?;
    let offset = optional_int(args, 2, 0) as usize;
    let compiled = match compile_fmt(&fmt, span) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    match compiled.unpack(&data, offset) {
        Ok((vals, _)) => {
            let out = vals.into_iter().map(unpack_to_value).collect();
            array_val(out)
        }
        Err(e) => Ok(binary_err(span, e)),
    }
}

// >>> nbinary.calcsize(">bh")
// => 4
fn nbinary_calcsize(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbinary_calcsize", span)?;
    let fmt = string_arg(args, 0, "nbinary_calcsize", span)?;
    match compile_fmt(&fmt, span) {
        Ok(c) => int_val(c.size() as i64),
        Err(e) => Ok(e),
    }
}

// >>> nbinary.unpack_from(">H", byte_array[0, 1, 2, 3], 2)
// => {values: [2], offset: 4}
fn nbinary_unpack_from(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nbinary_unpack_from", span)?;
    let fmt = string_arg(args, 0, "nbinary_unpack_from", span)?;
    let data = bytes_arg(args, 1, "nbinary_unpack_from", span)?;
    let offset = int_arg(args, 2, "nbinary_unpack_from", span)?;
    if offset < 0 {
        return Ok(binary_err(span, "offset must be >= 0"));
    }
    let compiled = match compile_fmt(&fmt, span) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    match compiled.unpack(&data, offset as usize) {
        Ok((vals, next)) => {
            let items: Vec<ValueRef> = vals.into_iter().map(unpack_to_value).collect();
            let mut map = HashMap::new();
            map.insert("values".to_string(), Value::Array(items).ref_cell());
            map.insert("offset".to_string(), Value::Int(next as i64).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(binary_err(span, e)),
    }
}

// >>> nbinary.pack_into(">I", byte_array[0, 0, 0, 0, 0], 1, 42)
// => 5
fn nbinary_pack_into(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() < 4 {
        return Err(RuntimeError::at(
            span,
            E3460_NBINARY_ARITY,
            format!(
                "nbinary_pack_into() expects at least 4 argument(s), got {}",
                args.len()
            ),
        ));
    }
    let fmt = string_arg(args, 0, "nbinary_pack_into", span)?;
    let mut buf = bytes_arg(args, 1, "nbinary_pack_into", span)?;
    let offset = int_arg(args, 2, "nbinary_pack_into", span)?;
    if offset < 0 {
        return Ok(binary_err(span, "offset must be >= 0"));
    }
    let compiled = match compile_fmt(&fmt, span) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    let mut values = Vec::new();
    for arg in &args[3..] {
        match value_to_pack(&arg.borrow(), span) {
            Ok(v) => values.push(v),
            Err(e) => return Ok(e),
        }
    }
    match compiled.pack_into(&mut buf, offset as usize, &values) {
        Ok(end) => int_val(end as i64),
        Err(e) => Ok(binary_err(span, e)),
    }
}

// >>> len(nbinary.iter_unpack(">H", byte_array[0, 1, 0, 2]))
// => 2
fn nbinary_iter_unpack(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbinary_iter_unpack", span)?;
    let fmt = string_arg(args, 0, "nbinary_iter_unpack", span)?;
    let data = bytes_arg(args, 1, "nbinary_iter_unpack", span)?;
    let compiled = match compile_fmt(&fmt, span) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    match compiled.iter_unpack(&data) {
        Ok(rows) => {
            let out: Vec<ValueRef> = rows
                .into_iter()
                .map(|(vals, off)| {
                    let items: Vec<ValueRef> = vals.into_iter().map(unpack_to_value).collect();
                    let mut map = HashMap::new();
                    map.insert("values".to_string(), Value::Array(items).ref_cell());
                    map.insert("offset".to_string(), Value::Int(off as i64).ref_cell());
                    Value::Object(map).ref_cell()
                })
                .collect();
            array_val(out)
        }
        Err(e) => Ok(binary_err(span, e)),
    }
}

// >>> nbinary.struct_format(">I").size
// => 4
fn nbinary_struct_format(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbinary_struct_format", span)?;
    let fmt = string_arg(args, 0, "nbinary_struct_format", span)?;
    let compiled = match compile_fmt(&fmt, span) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    let mut map = HashMap::new();
    map.insert("format".to_string(), Value::String(compiled.format.clone()).ref_cell());
    map.insert("size".to_string(), Value::Int(compiled.size() as i64).ref_cell());
    map.insert(
        "endian".to_string(),
        Value::String(compiled.endian.marker().to_string()).ref_cell(),
    );
    map.insert(
        "pack".to_string(),
        Value::NativeFunction(Rc::new(nbinary_struct_pack)).ref_cell(),
    );
    map.insert(
        "unpack".to_string(),
        Value::NativeFunction(Rc::new(nbinary_struct_unpack)).ref_cell(),
    );
    // Stash compiled format in a closure via thread-local keyed by format string is awkward;
    // struct pack/unpack builtins take (struct_obj, ...) — use object field "format" instead.
    Ok(Value::Object(map).ref_cell())
}

fn nbinary_struct_pack(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() < 2 {
        return Err(RuntimeError::at(
            span,
            E3460_NBINARY_ARITY,
            "struct.pack() expects struct object and at least one value".to_string(),
        ));
    }
    let fmt = struct_obj_format(args, 0, span)?;
    let mut pack_args = vec![Value::String(fmt).ref_cell()];
    pack_args.extend_from_slice(&args[1..]);
    nbinary_pack(&pack_args, span)
}

fn nbinary_struct_unpack(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbinary_struct_unpack", span)?;
    let fmt = struct_obj_format(args, 0, span)?;
    let mut unpack_args = vec![
        Value::String(fmt).ref_cell(),
        args[1].clone(),
    ];
    if args.len() > 2 {
        unpack_args.push(args[2].clone());
    }
    nbinary_unpack(&unpack_args, span)
}

fn struct_obj_format(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::Object(map) => match map.get("format") {
            Some(v) => match &*v.borrow() {
                Value::String(s) => Ok(s.clone()),
                other => Err(type_err(
                    span,
                    format!("struct object format must be string, got {}", other.type_name()),
                )),
            },
            None => Err(type_err(span, "struct object missing format field")),
        },
        other => Err(type_err(
            span,
            format!(
                "expected struct object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

// >>> nbinary.endian().little
// => "<"
fn nbinary_endian(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let mut map = HashMap::new();
    map.insert("native".to_string(), Value::String("@".into()).ref_cell());
    map.insert("standard".to_string(), Value::String("=".into()).ref_cell());
    map.insert("little".to_string(), Value::String("<".into()).ref_cell());
    map.insert("big".to_string(), Value::String(">".into()).ref_cell());
    map.insert("network".to_string(), Value::String("!".into()).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

// ---------------------------------------------------------------------------
// Varints
// ---------------------------------------------------------------------------

// >>> nbinary.uvarint_encode(300)
// => byte_array[172, 2]
fn nbinary_uvarint_encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbinary_uvarint_encode", span)?;
    let n = int_arg(args, 0, "nbinary_uvarint_encode", span)?;
    if n < 0 {
        return Ok(binary_err(span, "uvarint_encode expects non-negative int"));
    }
    bytes_val(uvarint_encode(n as u64))
}

// >>> nbinary.uvarint_decode(byte_array[172, 2])
// => {value: 300, offset: 2}
fn nbinary_uvarint_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nbinary_uvarint_decode", span)?;
    let data = bytes_arg(args, 0, "nbinary_uvarint_decode", span)?;
    let offset = optional_int(args, 1, 0) as usize;
    match uvarint_decode(&data, offset) {
        Ok(r) => {
            let mut map = HashMap::new();
            map.insert("value".to_string(), Value::Int(r.value as i64).ref_cell());
            map.insert(
                "offset".to_string(),
                Value::Int((offset + r.bytes_read) as i64).ref_cell(),
            );
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(binary_err(span, e)),
    }
}

// >>> nbinary.varint_encode(-1)
// => byte_array[1]
fn nbinary_varint_encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbinary_varint_encode", span)?;
    let n = int_arg(args, 0, "nbinary_varint_encode", span)?;
    bytes_val(varint_encode(n))
}

// >>> nbinary.varint_decode(byte_array[1])
// => {value: -1, offset: 1}
fn nbinary_varint_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nbinary_varint_decode", span)?;
    let data = bytes_arg(args, 0, "nbinary_varint_decode", span)?;
    let offset = optional_int(args, 1, 0) as usize;
    match varint_decode(&data, offset) {
        Ok((v, len)) => {
            let mut map = HashMap::new();
            map.insert("value".to_string(), Value::Int(v).ref_cell());
            map.insert("offset".to_string(), Value::Int((offset + len) as i64).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(binary_err(span, e)),
    }
}

// >>> nbinary.zigzag_encode(-1)
// => 1
fn nbinary_zigzag_encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbinary_zigzag_encode", span)?;
    let n = int_arg(args, 0, "nbinary_zigzag_encode", span)?;
    int_val(zigzag_encode(n) as i64)
}

// >>> nbinary.zigzag_decode(1)
// => -1
fn nbinary_zigzag_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbinary_zigzag_decode", span)?;
    let n = int_arg(args, 0, "nbinary_zigzag_decode", span)?;
    if n < 0 {
        return Ok(binary_err(span, "zigzag_decode expects non-negative int"));
    }
    int_val(zigzag_decode(n as u64))
}

// ---------------------------------------------------------------------------
// CRC
// ---------------------------------------------------------------------------

// >>> nbinary.crc32("123456789")
// => 3421780262
fn nbinary_crc32(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbinary_crc32", span)?;
    let data = bytes_arg(args, 0, "nbinary_crc32", span)?;
    int_val(crc32(&data) as i64)
}

// >>> nbinary.crc32_update(0, "123456789")
// => 3421780262
fn nbinary_crc32_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbinary_crc32_update", span)?;
    let crc = int_arg(args, 0, "nbinary_crc32_update", span)?;
    let data = bytes_arg(args, 1, "nbinary_crc32_update", span)?;
    int_val(crc32_update(crc as u32, &data) as i64)
}

// >>> type(nbinary.crc64("123456789"))
// => "int"
fn nbinary_crc64(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbinary_crc64", span)?;
    let data = bytes_arg(args, 0, "nbinary_crc64", span)?;
    int_val(crc64(&data) as i64)
}

// >>> nbinary.crc64_update(0, "123456789") > 0
// => true
fn nbinary_crc64_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbinary_crc64_update", span)?;
    let crc = int_arg(args, 0, "nbinary_crc64_update", span)?;
    let data = bytes_arg(args, 1, "nbinary_crc64_update", span)?;
    int_val(crc64_update(crc as u64, &data) as i64)
}

// ---------------------------------------------------------------------------
// BitString handles
// ---------------------------------------------------------------------------

// >>> type(nbinary.bits(8))
// => "int"
fn nbinary_bits(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nbinary_bits", span)?;
    let bit_len = int_arg(args, 0, "nbinary_bits", span)?;
    if bit_len < 0 {
        return Ok(binary_err(span, "bits length must be >= 0"));
    }
    let bs = if args.len() == 2 {
        let data = bytes_arg(args, 1, "nbinary_bits", span)?;
        BitString::from_bytes(&data, Some(bit_len as usize))
    } else {
        BitString::new(bit_len as usize)
    };
    let id = new_bits_handle();
    BITS.with(|map| map.borrow_mut().insert(id, bs));
    int_val(id)
}

// >>> nbinary.from_bytes(byte_array[160], 4)
// => (handle int)
fn nbinary_from_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nbinary_from_bytes", span)?;
    let data = bytes_arg(args, 0, "nbinary_from_bytes", span)?;
    let bit_len = if args.len() == 2 {
        Some(int_arg(args, 1, "nbinary_from_bytes", span)? as usize)
    } else {
        None
    };
    let bs = BitString::from_bytes(&data, bit_len);
    let id = new_bits_handle();
    BITS.with(|map| map.borrow_mut().insert(id, bs));
    int_val(id)
}

// >>> nbinary.bit_len(handle) >= 0
// => true
fn nbinary_bit_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbinary_bit_len", span)?;
    let id = int_arg(args, 0, "nbinary_bit_len", span)?;
    match with_bits(id, span, |bs| Ok(bs.len() as i64))? {
        Ok(n) => int_val(n),
        Err(e) => Ok(e),
    }
}

// >>> nbinary.get_bit(handle, 0) == false
// => true
fn nbinary_get_bit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbinary_get_bit", span)?;
    let id = int_arg(args, 0, "nbinary_get_bit", span)?;
    let pos = int_arg(args, 1, "nbinary_get_bit", span)?;
    if pos < 0 {
        return Ok(binary_err(span, "bit position must be >= 0"));
    }
    match with_bits(id, span, |bs| bs.get(pos as usize))? {
        Ok(b) => bool_val(b),
        Err(e) => Ok(e),
    }
}

// >>> nbinary.set_bit(handle, 0, true) == nil
// => true
fn nbinary_set_bit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nbinary_set_bit", span)?;
    let id = int_arg(args, 0, "nbinary_set_bit", span)?;
    let pos = int_arg(args, 1, "nbinary_set_bit", span)?;
    let val = match &*args[2].borrow() {
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        other => {
            return Err(type_err(
                span,
                format!("set_bit expects bool as argument 3, got {}", other.type_name()),
            ))
        }
    };
    if pos < 0 {
        return Ok(binary_err(span, "bit position must be >= 0"));
    }
    match with_bits(id, span, |bs| bs.set(pos as usize, val))? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nbinary.read_bits(handle, 4)
// => 10
fn nbinary_read_bits(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbinary_read_bits", span)?;
    let id = int_arg(args, 0, "nbinary_read_bits", span)?;
    let n = int_arg(args, 1, "nbinary_read_bits", span)?;
    if n < 0 {
        return Ok(binary_err(span, "read bit count must be >= 0"));
    }
    match with_bits(id, span, |bs| bs.read(n as usize))? {
        Ok(v) => int_val(v as i64),
        Err(e) => Ok(e),
    }
}

// >>> nbinary.write_bits(handle, 4, 10) == nil
// => true
fn nbinary_write_bits(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nbinary_write_bits", span)?;
    let id = int_arg(args, 0, "nbinary_write_bits", span)?;
    let n = int_arg(args, 1, "nbinary_write_bits", span)?;
    let val = int_arg(args, 2, "nbinary_write_bits", span)?;
    if n < 0 || val < 0 {
        return Ok(binary_err(span, "write_bits expects non-negative n and value"));
    }
    match with_bits(id, span, |bs| bs.write(n as usize, val as u64))? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nbinary.seek_bits(handle, 0) == nil
// => true
fn nbinary_seek_bits(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbinary_seek_bits", span)?;
    let id = int_arg(args, 0, "nbinary_seek_bits", span)?;
    let pos = int_arg(args, 1, "nbinary_seek_bits", span)?;
    if pos < 0 {
        return Ok(binary_err(span, "seek position must be >= 0"));
    }
    match with_bits(id, span, |bs| {
        bs.seek(pos as usize);
        Ok(())
    })? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nbinary.to_bytes(handle)
// => byte_array[...]
fn nbinary_to_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nbinary_to_bytes", span)?;
    let id = int_arg(args, 0, "nbinary_to_bytes", span)?;
    let pad = optional_bool(args, 1, true);
    match with_bits(id, span, |bs| Ok(bs.to_bytes(pad)))? {
        Ok(b) => bytes_val(b),
        Err(e) => Ok(e),
    }
}

// >>> nbinary.bits_hex(handle)
// => "a0"
fn nbinary_bits_hex(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbinary_bits_hex", span)?;
    let id = int_arg(args, 0, "nbinary_bits_hex", span)?;
    match with_bits(id, span, |bs| Ok(bs.hex()))? {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nbinary.release_bits(handle) == nil
// => true
fn nbinary_release_bits(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbinary_release_bits", span)?;
    let id = int_arg(args, 0, "nbinary_release_bits", span)?;
    BITS.with(|map| {
        map.borrow_mut().remove(&id);
    });
    Ok(Value::Nil.ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nbinary_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nbinary_fns![
    ("nbinary_pack", "pack", nbinary_pack),
    ("nbinary_unpack", "unpack", nbinary_unpack),
    ("nbinary_calcsize", "calcsize", nbinary_calcsize),
    ("nbinary_pack_into", "pack_into", nbinary_pack_into),
    ("nbinary_unpack_from", "unpack_from", nbinary_unpack_from),
    ("nbinary_iter_unpack", "iter_unpack", nbinary_iter_unpack),
    ("nbinary_struct_format", "struct_format", nbinary_struct_format),
    ("nbinary_endian", "endian", nbinary_endian),
    ("nbinary_uvarint_encode", "uvarint_encode", nbinary_uvarint_encode),
    ("nbinary_uvarint_decode", "uvarint_decode", nbinary_uvarint_decode),
    ("nbinary_varint_encode", "varint_encode", nbinary_varint_encode),
    ("nbinary_varint_decode", "varint_decode", nbinary_varint_decode),
    ("nbinary_zigzag_encode", "zigzag_encode", nbinary_zigzag_encode),
    ("nbinary_zigzag_decode", "zigzag_decode", nbinary_zigzag_decode),
    ("nbinary_crc32", "crc32", nbinary_crc32),
    ("nbinary_crc32_update", "crc32_update", nbinary_crc32_update),
    ("nbinary_crc64", "crc64", nbinary_crc64),
    ("nbinary_crc64_update", "crc64_update", nbinary_crc64_update),
    ("nbinary_bits", "bits", nbinary_bits),
    ("nbinary_from_bytes", "from_bytes", nbinary_from_bytes),
    ("nbinary_bit_len", "bit_len", nbinary_bit_len),
    ("nbinary_get_bit", "get_bit", nbinary_get_bit),
    ("nbinary_set_bit", "set_bit", nbinary_set_bit),
    ("nbinary_read_bits", "read_bits", nbinary_read_bits),
    ("nbinary_write_bits", "write_bits", nbinary_write_bits),
    ("nbinary_seek_bits", "seek_bits", nbinary_seek_bits),
    ("nbinary_to_bytes", "to_bytes", nbinary_to_bytes),
    ("nbinary_bits_hex", "bits_hex", nbinary_bits_hex),
    ("nbinary_release_bits", "release_bits", nbinary_release_bits),
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

pub const MODULE_NAME: &str = "nbinary";
pub const MODULE_PATHS: &[&str] = &["nbinary", "std/nbinary"];

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

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn bytes(v: &[u8]) -> ValueRef {
        Value::ByteArray(v.to_vec()).ref_cell()
    }

    fn expect_bytes(r: NiaoResult<ValueRef>) -> Vec<u8> {
        match &*r.unwrap().borrow() {
            Value::ByteArray(b) => b.clone(),
            other => panic!("expected bytes, got {other:?}"),
        }
    }

    fn expect_int(r: NiaoResult<ValueRef>) -> i64 {
        match &*r.unwrap().borrow() {
            Value::Int(n) => *n,
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[test]
    fn pack_unpack_uint32_be() {
        let packed = expect_bytes(nbinary_pack(&[s(">I"), i(0x0102_0304)], span()));
        assert_eq!(packed, vec![1, 2, 3, 4]);
        let vals = nbinary_unpack(&[s(">I"), bytes(&packed)], span()).unwrap();
        match &*vals.borrow() {
            Value::Array(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(expect_int(Ok(items[0].clone())), 0x0102_0304);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn calcsize_aligned() {
        assert_eq!(expect_int(nbinary_calcsize(&[s(">bh")], span())), 4);
    }

    #[test]
    fn varint_roundtrip() {
        let enc = expect_bytes(nbinary_varint_encode(&[i(-1)], span()));
        let dec = nbinary_varint_decode(&[bytes(&enc)], span()).unwrap();
        match &*dec.borrow() {
            Value::Object(m) => {
                assert_eq!(
                    expect_int(Ok(m.get("value").unwrap().clone())),
                    -1
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn crc32_known() {
        assert_eq!(
            expect_int(nbinary_crc32(&[bytes(b"123456789")], span())),
            0xCBF4_3926_i64
        );
    }

    #[test]
    fn bits_read_write() {
        let h = expect_int(nbinary_bits(&[i(8)], span()));
        nbinary_write_bits(&[i(h), i(4), i(0b1010)], span()).unwrap();
        nbinary_seek_bits(&[i(h), i(0)], span()).unwrap();
        assert_eq!(expect_int(nbinary_read_bits(&[i(h), i(4)], span())), 10);
        nbinary_release_bits(&[i(h)], span()).unwrap();
    }
}
