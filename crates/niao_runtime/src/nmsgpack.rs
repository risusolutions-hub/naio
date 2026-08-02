//! Native nmsgpack standard library — MessagePack encode/decode, streaming.
//! (~msgpack subset).
//!
//! Import with `import "nmsgpack"` (or `import "std/nmsgpack"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_bignum::BigInt;
use niao_errors::codes;
use niao_msgpack::{
    is_valid, pack, pack_all, unpack, unpack_all, MsgValue, MsgpackError, PackOptions, Packer,
    UnpackOptions, Unpacker, MAX_BYTES,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::rc::Rc;

thread_local! {
    static PACKERS: RefCell<HashMap<i64, Packer>> = RefCell::new(HashMap::new());
    static UNPACKERS: RefCell<HashMap<i64, Unpacker>> = RefCell::new(HashMap::new());
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

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4322_NMSGPACK_TYPE, msg.into())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4320_NMSGPACK_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nmsgpack_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4321_NMSGPACK_ERROR, "nmsgpack_error", msg.into(), span)
}

fn invalid_handle(span: Span) -> ValueRef {
    error_value(
        codes::E4323_NMSGPACK_INVALID_HANDLE,
        "nmsgpack_error",
        "invalid stream handle",
        span,
    )
}

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects byte[] or string as argument {}, got {}",
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

fn pack_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> PackOptions {
    PackOptions {
        use_bin_type: bool_field(map, "use_bin_type", false),
        use_single_float: bool_field(map, "use_single_float", false),
        timestamp: bool_field(map, "timestamp", true),
        bigint_as_string: bool_field(map, "bigint_as_string", true),
    }
}

fn unpack_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> UnpackOptions {
    UnpackOptions {
        strict_map_key: bool_field(map, "strict_map_key", true),
        raw: bool_field(map, "raw", false),
        timestamp: bool_field(map, "timestamp", true),
        bigint_as_string: bool_field(map, "bigint_as_string", true),
        max_depth: int_field(map, "max_depth", 512).max(1) as usize,
    }
}

fn map_err(span: Span, err: MsgpackError) -> ValueRef {
    nmsgpack_err(span, err.message())
}

fn bytes_val(b: Vec<u8>) -> NiaoResult<ValueRef> {
    Ok(Value::ByteArray(b).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

// ---------------------------------------------------------------------------
// MsgValue ↔ Niao Value bridge
// ---------------------------------------------------------------------------

fn msg_to_niao(v: MsgValue) -> Value {
    match v {
        MsgValue::Nil => Value::Nil,
        MsgValue::Bool(b) => Value::Bool(b),
        MsgValue::Int(n) => Value::Int(n),
        MsgValue::Uint(n) if n <= i64::MAX as u64 => Value::Int(n as i64),
        MsgValue::Uint(n) => Value::BigInt(BigInt::from(n)),
        MsgValue::BigInt(n) => Value::BigInt(n),
        MsgValue::Float(f) => {
            if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                Value::Int(f as i64)
            } else {
                Value::Float(f)
            }
        }
        MsgValue::String(s) => Value::String(s),
        MsgValue::Binary(b) => Value::ByteArray(b),
        MsgValue::Array(items) => {
            let out: Vec<ValueRef> = items.into_iter().map(|i| msg_to_niao(i).ref_cell()).collect();
            Value::Array(out)
        }
        MsgValue::Map(pairs) => {
            let mut out = HashMap::with_capacity(pairs.len());
            for (k, v) in pairs {
                let key = match k {
                    MsgValue::String(s) => s,
                    other => format!("{other:?}"),
                };
                out.insert(key, msg_to_niao(v).ref_cell());
            }
            Value::Object(out)
        }
        MsgValue::Ext { code, data } => {
            let mut out = HashMap::new();
            out.insert("code".into(), Value::Int(code as i64).ref_cell());
            out.insert("data".into(), Value::ByteArray(data).ref_cell());
            Value::Object(out)
        }
        MsgValue::Timestamp { sec, nsec } => {
            let mut out = HashMap::new();
            out.insert("sec".into(), Value::Int(sec).ref_cell());
            out.insert("nsec".into(), Value::Int(nsec as i64).ref_cell());
            Value::Object(out)
        }
    }
}

fn is_ext_object(map: &HashMap<String, ValueRef>) -> Option<(i8, Vec<u8>)> {
    let code_v = map.get("code")?;
    let data_v = map.get("data")?;
    let code = match &*code_v.borrow() {
        Value::Int(n) if *n >= i8::MIN as i64 && *n <= i8::MAX as i64 => *n as i8,
        _ => return None,
    };
    let data = match &*data_v.borrow() {
        Value::ByteArray(b) => b.clone(),
        Value::String(s) => s.as_bytes().to_vec(),
        _ => return None,
    };
    Some((code, data))
}

fn is_timestamp_object(map: &HashMap<String, ValueRef>) -> Option<(i64, u32)> {
    let sec = map
        .get("sec")
        .or_else(|| map.get("seconds"))
        .and_then(|v| match &*v.borrow() {
            Value::Int(n) => Some(*n),
            _ => None,
        })?;
    let nsec = map
        .get("nsec")
        .or_else(|| map.get("nanoseconds"))
        .and_then(|v| match &*v.borrow() {
            Value::Int(n) if *n >= 0 => Some(*n as u32),
            _ => None,
        })
        .unwrap_or(0);
    Some((sec, nsec))
}

fn niao_to_msg(v: &Value, span: Span) -> NiaoResult<MsgValue> {
    match v {
        Value::Nil => Ok(MsgValue::Nil),
        Value::Bool(b) => Ok(MsgValue::Bool(*b)),
        Value::Int(n) => Ok(MsgValue::Int(*n)),
        Value::BigInt(n) => Ok(MsgValue::BigInt(n.clone())),
        Value::Float(f) => Ok(MsgValue::Float(*f)),
        Value::String(s) => Ok(MsgValue::String(s.clone())),
        Value::ByteArray(b) => Ok(MsgValue::Binary(b.clone())),
        Value::IntArray(items) => Ok(MsgValue::Array(
            items.iter().map(|&n| MsgValue::Int(n)).collect(),
        )),
        Value::FloatArray(items) => Ok(MsgValue::Array(
            items.iter().map(|&f| MsgValue::Float(f)).collect(),
        )),
        Value::BoolArray(items) => Ok(MsgValue::Array(
            items.iter().map(|&b| MsgValue::Bool(b != 0)).collect(),
        )),
        Value::StringArray(items) => {
            let mut seq = Vec::with_capacity(items.len());
            for i in 0..items.len() {
                seq.push(MsgValue::String(items.get(i).unwrap_or_default()));
            }
            Ok(MsgValue::Array(seq))
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for slot in items {
                out.push(niao_to_msg(&slot.borrow(), span)?);
            }
            Ok(MsgValue::Array(out))
        }
        Value::Object(map) => {
            if let Some((code, data)) = is_ext_object(map) {
                return Ok(MsgValue::Ext { code, data });
            }
            if let Some((sec, nsec)) = is_timestamp_object(map) {
                return Ok(MsgValue::Timestamp { sec, nsec });
            }
            let mut pairs = Vec::with_capacity(map.len());
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for k in keys {
                pairs.push((
                    MsgValue::String(k.clone()),
                    niao_to_msg(&map[k].borrow(), span)?,
                ));
            }
            Ok(MsgValue::Map(pairs))
        }
        other => Err(type_err(
            span,
            format!("nmsgpack: cannot encode value of type {}", other.type_name()),
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> len(nmsgpack.pack({a: 1}))
// => 2
fn nmsgpack_pack(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmsgpack_pack", span)?;
    let msg = niao_to_msg(&args[0].borrow(), span)?;
    let opts = pack_opts_from_map(optional_object_arg(args, 1).as_ref());
    match pack(&msg, &opts) {
        Ok(b) => bytes_val(b),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nmsgpack.unpack(nmsgpack.pack({x: 1})).x
// => 1
fn nmsgpack_unpack(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmsgpack_unpack", span)?;
    let data = bytes_arg(args, 0, "nmsgpack_unpack", span)?;
    if data.len() > MAX_BYTES {
        return Ok(nmsgpack_err(
            span,
            format!("input size {} exceeds limit {MAX_BYTES}", data.len()),
        ));
    }
    let opts = unpack_opts_from_map(optional_object_arg(args, 1).as_ref());
    match unpack(&data, &opts) {
        Ok(v) => Ok(msg_to_niao(v).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(nmsgpack.pack_all([1, 2, 3]))
// => 3
fn nmsgpack_pack_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmsgpack_pack_all", span)?;
    let arg0 = args[0].borrow();
    let items = match &*arg0 {
        Value::Array(items) => items,
        other => {
            return Err(type_err(
                span,
                format!(
                    "nmsgpack_pack_all() expects an array as argument 1, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let opts = pack_opts_from_map(optional_object_arg(args, 1).as_ref());
    let mut values = Vec::with_capacity(items.len());
    for slot in items {
        values.push(niao_to_msg(&slot.borrow(), span)?);
    }
    match pack_all(&values, &opts) {
        Ok(b) => bytes_val(b),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(nmsgpack.unpack_all(nmsgpack.pack_all([1, 2])))
// => 2
fn nmsgpack_unpack_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmsgpack_unpack_all", span)?;
    let data = bytes_arg(args, 0, "nmsgpack_unpack_all", span)?;
    let opts = unpack_opts_from_map(optional_object_arg(args, 1).as_ref());
    match unpack_all(&data, &opts) {
        Ok(values) => {
            let out: Vec<ValueRef> = values.into_iter().map(|v| msg_to_niao(v).ref_cell()).collect();
            Ok(Value::Array(out).ref_cell())
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nmsgpack.valid(nmsgpack.pack(true))
// => true
fn nmsgpack_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmsgpack_valid", span)?;
    let data = bytes_arg(args, 0, "nmsgpack_valid", span)?;
    bool_val(is_valid(&data))
}

// >>> nmsgpack.pack_file("out.msg", {ok: true})
// => true
fn nmsgpack_pack_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmsgpack_pack_file", span)?;
    let path = string_arg(args, 0, "nmsgpack_pack_file", span)?;
    let msg = niao_to_msg(&args[1].borrow(), span)?;
    let opts = pack_opts_from_map(optional_object_arg(args, 2).as_ref());
    let bytes = match pack(&msg, &opts) {
        Ok(b) => b,
        Err(e) => return Ok(map_err(span, e)),
    };
    if let Err(e) = fs::write(&path, &bytes) {
        return Ok(nmsgpack_err(
            span,
            format!("nmsgpack_pack_file: cannot write '{path}': {e}"),
        ));
    }
    bool_val(true)
}

// >>> nmsgpack.unpack_file(path).ok
// => true
fn nmsgpack_unpack_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmsgpack_unpack_file", span)?;
    let path = string_arg(args, 0, "nmsgpack_unpack_file", span)?;
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            return Ok(nmsgpack_err(
                span,
                format!("nmsgpack_unpack_file: cannot read '{path}': {e}"),
            ));
        }
    };
    if bytes.len() > MAX_BYTES {
        return Ok(nmsgpack_err(
            span,
            format!("file size {} exceeds limit {MAX_BYTES}", bytes.len()),
        ));
    }
    let opts = unpack_opts_from_map(optional_object_arg(args, 1).as_ref());
    match unpack(&bytes, &opts) {
        Ok(v) => Ok(msg_to_niao(v).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nmsgpack.ext(1, [0xCA, 0xFE]).code
// => 1
fn nmsgpack_ext(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nmsgpack_ext", span)?;
    let code = int_arg(args, 0, "nmsgpack_ext", span)?;
    if code < i8::MIN as i64 || code > i8::MAX as i64 {
        return Ok(nmsgpack_err(span, "extension code must fit in signed byte"));
    }
    let data = bytes_arg(args, 1, "nmsgpack_ext", span)?;
    let mut map = HashMap::new();
    map.insert("code".into(), Value::Int(code).ref_cell());
    map.insert("data".into(), Value::ByteArray(data).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

// >>> nmsgpack.timestamp(100, 5).sec
// => 100
fn nmsgpack_timestamp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmsgpack_timestamp", span)?;
    let sec = int_arg(args, 0, "nmsgpack_timestamp", span)?;
    let nsec = if args.len() > 1 {
        int_arg(args, 1, "nmsgpack_timestamp", span)? as u32
    } else {
        0
    };
    let mut map = HashMap::new();
    map.insert("sec".into(), Value::Int(sec).ref_cell());
    map.insert("nsec".into(), Value::Int(nsec as i64).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

// >>> nmsgpack.packer().handle > 0
// => true
fn nmsgpack_packer(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nmsgpack_packer", span)?;
    let opts = pack_opts_from_map(optional_object_arg(args, 0).as_ref());
    let id = new_handle();
    PACKERS.with(|m| {
        m.borrow_mut().insert(id, Packer::new(opts));
    });
    let mut map = HashMap::new();
    map.insert("handle".into(), int_val(id)?);
    Ok(Value::Object(map).ref_cell())
}

// >>> nmsgpack.packer_pack(p.handle, 42)
// => true
fn nmsgpack_packer_pack(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nmsgpack_packer_pack", span)?;
    let id = int_arg(args, 0, "nmsgpack_packer_pack", span)?;
    let msg = niao_to_msg(&args[1].borrow(), span)?;
    let result = PACKERS.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(p) => p.pack(&msg).map_err(|e| e.message()),
            None => Err("invalid packer handle".into()),
        }
    });
    match result {
        Ok(()) => bool_val(true),
        Err(msg) => Ok(if msg.contains("invalid") {
            invalid_handle(span)
        } else {
            nmsgpack_err(span, msg)
        }),
    }
}

// >>> len(nmsgpack.packer_finish(p.handle))
// => 1
fn nmsgpack_packer_finish(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmsgpack_packer_finish", span)?;
    let id = int_arg(args, 0, "nmsgpack_packer_finish", span)?;
    let bytes = PACKERS.with(|m| {
        let mut m = m.borrow_mut();
        m.remove(&id).map(|p| p.finish())
    });
    match bytes {
        Some(b) => bytes_val(b),
        None => Ok(invalid_handle(span)),
    }
}

// >>> nmsgpack.packer_bytes(p.handle)
fn nmsgpack_packer_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmsgpack_packer_bytes", span)?;
    let id = int_arg(args, 0, "nmsgpack_packer_bytes", span)?;
    PACKERS.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(p) => bytes_val(p.bytes().to_vec()),
            None => Ok(invalid_handle(span)),
        }
    })
}

// >>> nmsgpack.packer_reset(p.handle)
fn nmsgpack_packer_reset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmsgpack_packer_reset", span)?;
    let id = int_arg(args, 0, "nmsgpack_packer_reset", span)?;
    PACKERS.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(p) => {
                p.reset();
                bool_val(true)
            }
            None => Ok(invalid_handle(span)),
        }
    })
}

// >>> nmsgpack.unpacker().handle > 0
fn nmsgpack_unpacker(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 2, "nmsgpack_unpacker", span)?;
    let opts = unpack_opts_from_map(optional_object_arg(args, 0).as_ref());
    let initial = if args.len() > 1 {
        Some(bytes_arg(args, 1, "nmsgpack_unpacker", span)?)
    } else {
        None
    };
    let id = new_handle();
    let mut u = Unpacker::new(opts);
    if let Some(data) = initial {
        if let Err(e) = u.feed(&data) {
            return Ok(map_err(span, e));
        }
    }
    UNPACKERS.with(|m| {
        m.borrow_mut().insert(id, u);
    });
    let mut map = HashMap::new();
    map.insert("handle".into(), int_val(id)?);
    Ok(Value::Object(map).ref_cell())
}

// >>> nmsgpack.unpacker_feed(u.handle, chunk)
fn nmsgpack_unpacker_feed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nmsgpack_unpacker_feed", span)?;
    let id = int_arg(args, 0, "nmsgpack_unpacker_feed", span)?;
    let chunk = bytes_arg(args, 1, "nmsgpack_unpacker_feed", span)?;
    let result = UNPACKERS.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(u) => u.feed(&chunk).map_err(|e| e.message()),
            None => Err("invalid unpacker handle".into()),
        }
    });
    match result {
        Ok(()) => bool_val(true),
        Err(msg) => Ok(if msg.contains("invalid") {
            invalid_handle(span)
        } else {
            nmsgpack_err(span, msg)
        }),
    }
}

// >>> nmsgpack.unpacker_next(u.handle)
fn nmsgpack_unpacker_next(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmsgpack_unpacker_next", span)?;
    let id = int_arg(args, 0, "nmsgpack_unpacker_next", span)?;
    let result = UNPACKERS.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(u) => u.next().map_err(|e| e.message()),
            None => Err("invalid unpacker handle".into()),
        }
    });
    match result {
        Ok(Some(v)) => Ok(msg_to_niao(v).ref_cell()),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(msg) => Ok(if msg.contains("invalid") {
            invalid_handle(span)
        } else {
            nmsgpack_err(span, msg)
        }),
    }
}

// >>> nmsgpack.unpacker_tell(u.handle)
fn nmsgpack_unpacker_tell(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmsgpack_unpacker_tell", span)?;
    let id = int_arg(args, 0, "nmsgpack_unpacker_tell", span)?;
    UNPACKERS.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(u) => int_val(u.tell() as i64),
            None => Ok(invalid_handle(span)),
        }
    })
}

// >>> nmsgpack.unpacker_reset(u.handle)
fn nmsgpack_unpacker_reset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmsgpack_unpacker_reset", span)?;
    let id = int_arg(args, 0, "nmsgpack_unpacker_reset", span)?;
    UNPACKERS.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(u) => {
                u.reset();
                bool_val(true)
            }
            None => Ok(invalid_handle(span)),
        }
    })
}

// Python msgpack aliases
fn nmsgpack_packb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nmsgpack_pack(args, span)
}

fn nmsgpack_unpackb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nmsgpack_unpack(args, span)
}

fn nmsgpack_dumps(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nmsgpack_pack(args, span)
}

fn nmsgpack_loads(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nmsgpack_unpack(args, span)
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! nmsgpack_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nmsgpack_fns![
    ("nmsgpack_pack", "pack", nmsgpack_pack),
    ("nmsgpack_unpack", "unpack", nmsgpack_unpack),
    ("nmsgpack_pack_all", "pack_all", nmsgpack_pack_all),
    ("nmsgpack_unpack_all", "unpack_all", nmsgpack_unpack_all),
    ("nmsgpack_valid", "valid", nmsgpack_valid),
    ("nmsgpack_pack_file", "pack_file", nmsgpack_pack_file),
    ("nmsgpack_unpack_file", "unpack_file", nmsgpack_unpack_file),
    ("nmsgpack_ext", "ext", nmsgpack_ext),
    ("nmsgpack_timestamp", "timestamp", nmsgpack_timestamp),
    ("nmsgpack_packer", "packer", nmsgpack_packer),
    ("nmsgpack_packer_pack", "packer_pack", nmsgpack_packer_pack),
    ("nmsgpack_packer_finish", "packer_finish", nmsgpack_packer_finish),
    ("nmsgpack_packer_bytes", "packer_bytes", nmsgpack_packer_bytes),
    ("nmsgpack_packer_reset", "packer_reset", nmsgpack_packer_reset),
    ("nmsgpack_unpacker", "unpacker", nmsgpack_unpacker),
    ("nmsgpack_unpacker_feed", "unpacker_feed", nmsgpack_unpacker_feed),
    ("nmsgpack_unpacker_next", "unpacker_next", nmsgpack_unpacker_next),
    ("nmsgpack_unpacker_tell", "unpacker_tell", nmsgpack_unpacker_tell),
    ("nmsgpack_unpacker_reset", "unpacker_reset", nmsgpack_unpacker_reset),
    ("nmsgpack_packb", "packb", nmsgpack_packb),
    ("nmsgpack_unpackb", "unpackb", nmsgpack_unpackb),
    ("nmsgpack_dumps", "dumps", nmsgpack_dumps),
    ("nmsgpack_loads", "loads", nmsgpack_loads),
];

pub const MODULE_NAME: &str = "nmsgpack";
pub const MODULE_PATHS: &[&str] = &["nmsgpack", "std/nmsgpack"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn pack_unpack_roundtrip() {
        let mut obj = HashMap::new();
        obj.insert("a".into(), Value::Int(1).ref_cell());
        obj.insert("b".into(), Value::String("hi".into()).ref_cell());
        let args = vec![Value::Object(obj).ref_cell()];
        let bytes = nmsgpack_pack(&args, span()).unwrap();
        match &*bytes.borrow() {
            Value::ByteArray(b) => assert!(!b.is_empty()),
            other => panic!("expected bytes, got {other:?}"),
        }
        let out = nmsgpack_unpack(&[bytes], span()).unwrap();
        match &*out.borrow() {
            Value::Object(m) => assert!(m.contains_key("a")),
            other => panic!("expected object, got {other:?}"),
        }
    }
}
