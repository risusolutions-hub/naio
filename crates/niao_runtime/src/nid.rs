//! Native `nid` standard library — ID generation: ULID, UUIDv7, nanoid, snowflake,
//! hashids (~uuid6, ulid-py; extends `codec` UUID).
//!
//! Import with `import "nid"` (or `import "std/nid"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_id::{
    hashids::Hashids, nanoid_bulk, nanoid_size, nanoid_with, uuid4, uuid6, uuid6_from_timestamp,
    uuid7, HashidsError, MonotonicUlid, NanoidError, SnowflakeError, SnowflakeGenerator,
    Ulid, UlidError, NANOID_DEFAULT_ALPHABET, NANOID_DEFAULT_SIZE, SNOWFLAKE_DEFAULT_EPOCH,
    HASHIDS_DEFAULT_ALPHABET, MAX_DATACENTER_ID, MAX_WORKER_ID,
};
use niao_id::{snowflake_parse, uuid_from_bytes, uuid_is_valid, uuid_parse, uuid_timestamp_ms, uuid_to_bytes};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

const E3534_NID_ARITY: u32 = codes::E3534_NID_ARITY;
const E3535_NID_ERROR: u32 = codes::E3535_NID_ERROR;
const E3536_NID_TYPE: u32 = codes::E3536_NID_TYPE;
const E3537_NID_INVALID: u32 = codes::E3537_NID_INVALID;

// ---------------------------------------------------------------------------
// Handles
// ---------------------------------------------------------------------------

enum NidHandle {
    UlidMaker(MonotonicUlid),
    Snowflake(Arc<SnowflakeGenerator>),
    Hashids(Hashids),
}

thread_local! {
    static HANDLES: RefCell<HashMap<i64, NidHandle>> = RefCell::new(HashMap::new());
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

fn register(handle: NidHandle) -> i64 {
    let id = new_handle();
    HANDLES.with(|m| m.borrow_mut().insert(id, handle));
    id
}

fn with_handle<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut NidHandle) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    HANDLES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(h) => Ok(Ok(f(h))),
            None => Ok(Err(error_value(
                E3537_NID_INVALID,
                "nid_error",
                format!("invalid or closed nid handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3536_NID_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3534_NID_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3534_NID_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nid_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3535_NID_ERROR, "nid_error", msg.into(), span)
}

fn str_val(s: impl Into<String>) -> ValueRef {
    Value::String(s.into()).ref_cell()
}

fn int_val(n: i64) -> ValueRef {
    Value::Int(n).ref_cell()
}

fn bool_val(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
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

fn optional_string(args: &[ValueRef], idx: usize, default: &str) -> String {
    if args.len() <= idx {
        return default.to_string();
    }
    match &*args[idx].borrow() {
        Value::String(s) => s.clone(),
        _ => default.to_string(),
    }
}

fn int_array_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<i64>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::Int(n) => out.push(*n),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects int array elements, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        Value::IntArray(v) => Ok(v.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn u64_array_from_args(args: &[ValueRef], start: usize, span: Span, name: &str) -> NiaoResult<Result<Vec<u64>, ValueRef>> {
    let mut out = Vec::new();
    for (i, arg) in args.iter().skip(start).enumerate() {
        match &*arg.borrow() {
            Value::Int(n) => {
                if *n < 0 {
                    return Ok(Err(nid_err(
                        span,
                        format!("{name}() number {} must be >= 0", i + start + 1),
                    )));
                }
                out.push(*n as u64);
            }
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "{name}() expects int numbers, got {} at position {}",
                        other.type_name(),
                        i + start + 1
                    ),
                ));
            }
        }
    }
    Ok(Ok(out))
}

fn bytes_from_int_array(items: &[i64], span: Span, name: &str) -> Result<Vec<u8>, ValueRef> {
    let mut out = Vec::with_capacity(items.len());
    for &n in items {
        if !(0..=255).contains(&n) {
            return Err(nid_err(span, format!("{name}() bytes must be 0..=255")));
        }
        out.push(n as u8);
    }
    Ok(out)
}

fn int_array_from_bytes(bytes: &[u8]) -> ValueRef {
    Value::Array(bytes.iter().map(|&b| Value::Int(b as i64).ref_cell()).collect()).ref_cell()
}

fn handle_id_from_arg(args: &[ValueRef], idx: usize, span: Span, name: &str) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Object(map) => match map.get("id") {
            Some(v) => match &*v.borrow() {
                Value::Int(n) => Ok(*n),
                other => Err(type_err(
                    span,
                    format!("{name}() handle id must be int, got {}", other.type_name()),
                )),
            },
            None => Err(type_err(span, format!("{name}() object missing id field"))),
        },
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects handle object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn handle_object(id: i64, kind: &str, methods: HashMap<String, ValueRef>) -> ValueRef {
    let mut map = methods;
    map.insert("id".to_string(), int_val(id));
    map.insert("kind".to_string(), str_val(kind));
    Value::Object(map).ref_cell()
}

fn map_ulid_err(e: UlidError) -> String {
    e.to_string()
}

fn map_nanoid_err(e: NanoidError) -> String {
    e.to_string()
}

fn map_hashids_err(e: HashidsError) -> String {
    e.to_string()
}

fn map_snowflake_err(e: SnowflakeError) -> String {
    e.to_string()
}

// ---------------------------------------------------------------------------
// UUID
// ---------------------------------------------------------------------------

// >>> import "nid"
// >>> len(nid.uuid4())
// 36
fn nid_uuid4(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(str_val(uuid4().to_string()))
}

// >>> len(nid.uuid7())
// 36
fn nid_uuid7(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(str_val(uuid7().to_string()))
}

// >>> type(nid.uuid6())
// "string"
fn nid_uuid6(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nid_uuid6", span)?;
    if args.is_empty() {
        return Ok(str_val(uuid6().to_string()));
    }
    let ts = int_arg(args, 0, "nid_uuid6", span)?;
    if ts < 0 {
        return Ok(nid_err(span, "timestamp must be >= 0"));
    }
    Ok(str_val(uuid6_from_timestamp(ts as u64).to_string()))
}

// >>> nid.uuid_is_valid(nid.uuid4())
// true
fn nid_uuid_is_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nid_uuid_is_valid", span)?;
    let s = string_arg(args, 0, "nid_uuid_is_valid", span)?;
    Ok(bool_val(uuid_is_valid(&s)))
}

// >>> nid.uuid_version(nid.uuid7())
// 7
fn nid_uuid_version(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nid_uuid_version", span)?;
    let s = string_arg(args, 0, "nid_uuid_version", span)?;
    match uuid_parse(&s) {
        Ok(u) => Ok(int_val(u.version() as i64)),
        Err(e) => Ok(nid_err(span, e.to_string())),
    }
}

// >>> len(nid.uuid_bytes(nid.uuid4()))
// 16
fn nid_uuid_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nid_uuid_bytes", span)?;
    let s = string_arg(args, 0, "nid_uuid_bytes", span)?;
    match uuid_parse(&s) {
        Ok(u) => Ok(int_array_from_bytes(&uuid_to_bytes(&u))),
        Err(e) => Ok(nid_err(span, e.to_string())),
    }
}

// >>> nid.uuid_from_bytes(nid.uuid_bytes(nid.uuid4())) == nid.uuid4()
// true
fn nid_uuid_from_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nid_uuid_from_bytes", span)?;
    let items = int_array_arg(args, 0, "nid_uuid_from_bytes", span)?;
    let bytes = match bytes_from_int_array(&items, span, "nid_uuid_from_bytes") {
        Ok(b) => b,
        Err(e) => return Ok(e),
    };
    match uuid_from_bytes(&bytes) {
        Ok(u) => Ok(str_val(u.to_string())),
        Err(e) => Ok(nid_err(span, e.to_string())),
    }
}

// >>> type(nid.uuid_timestamp(nid.uuid7()))
// "int"
fn nid_uuid_timestamp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nid_uuid_timestamp", span)?;
    let s = string_arg(args, 0, "nid_uuid_timestamp", span)?;
    match uuid_parse(&s) {
        Ok(u) => match uuid_timestamp_ms(&u) {
            Some(ts) => Ok(int_val(ts as i64)),
            None => Ok(Value::Nil.ref_cell()),
        },
        Err(e) => Ok(nid_err(span, e.to_string())),
    }
}

// >>> nid.uuid_parse(nid.uuid4()).ok
// true
fn nid_uuid_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nid_uuid_parse", span)?;
    let s = string_arg(args, 0, "nid_uuid_parse", span)?;
    match uuid_parse(&s) {
        Ok(u) => {
            let mut map = HashMap::new();
            map.insert("ok".to_string(), bool_val(true));
            map.insert("value".to_string(), str_val(u.to_string()));
            map.insert("version".to_string(), int_val(u.version() as i64));
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => {
            let mut map = HashMap::new();
            map.insert("ok".to_string(), bool_val(false));
            map.insert("error".to_string(), str_val(e.to_string()));
            Ok(Value::Object(map).ref_cell())
        }
    }
}

// ---------------------------------------------------------------------------
// ULID
// ---------------------------------------------------------------------------

// >>> len(nid.ulid())
// 26
fn nid_ulid(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(str_val(Ulid::new().to_string()))
}

// >>> nid.ulid_is_valid(nid.ulid())
// true
fn nid_ulid_is_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nid_ulid_is_valid", span)?;
    let s = string_arg(args, 0, "nid_ulid_is_valid", span)?;
    Ok(bool_val(Ulid::is_valid(&s)))
}

// >>> type(nid.ulid_timestamp(nid.ulid()))
// "int"
fn nid_ulid_timestamp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nid_ulid_timestamp", span)?;
    let s = string_arg(args, 0, "nid_ulid_timestamp", span)?;
    match Ulid::parse(&s) {
        Ok(u) => Ok(int_val(u.timestamp_ms() as i64)),
        Err(e) => Ok(nid_err(span, map_ulid_err(e))),
    }
}

// >>> nid.ulid_parse(nid.ulid()).ok
// true
fn nid_ulid_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nid_ulid_parse", span)?;
    let s = string_arg(args, 0, "nid_ulid_parse", span)?;
    match Ulid::parse(&s) {
        Ok(u) => {
            let mut map = HashMap::new();
            map.insert("ok".to_string(), bool_val(true));
            map.insert("value".to_string(), str_val(u.to_string()));
            map.insert("timestamp".to_string(), int_val(u.timestamp_ms() as i64));
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => {
            let mut map = HashMap::new();
            map.insert("ok".to_string(), bool_val(false));
            map.insert("error".to_string(), str_val(map_ulid_err(e)));
            Ok(Value::Object(map).ref_cell())
        }
    }
}

// >>> type(nid.ulid_maker().next())
// "string"
fn nid_ulid_maker(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nid_ulid_maker", span)?;
    let _ = args;
    let id = register(NidHandle::UlidMaker(MonotonicUlid::new()));
    let mut methods = HashMap::new();
    methods.insert(
        "next".to_string(),
        Value::NativeFunction(Rc::new(nid_ulid_maker_next)).ref_cell(),
    );
    Ok(handle_object(id, "ulid_maker", methods))
}

fn nid_ulid_maker_next(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ulid_maker.next", span)?;
    let id = handle_id_from_arg(args, 0, span, "ulid_maker.next")?;
    match with_handle(id, span, |h| {
        if let NidHandle::UlidMaker(gen) = h {
            gen.next().to_string()
        } else {
            String::new()
        }
    })? {
        Ok(s) if !s.is_empty() => Ok(str_val(s)),
        Ok(_) => Ok(nid_err(span, "invalid ulid_maker handle")),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Nanoid
// ---------------------------------------------------------------------------

// >>> len(nid.nanoid())
// 21
fn nid_nanoid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 2, "nid_nanoid", span)?;
    let size = if args.is_empty() {
        NANOID_DEFAULT_SIZE
    } else {
        let n = int_arg(args, 0, "nid_nanoid", span)?;
        if n <= 0 {
            return Ok(nid_err(span, "size must be > 0"));
        }
        n as usize
    };
    if args.len() == 2 {
        let alphabet = string_arg(args, 1, "nid_nanoid", span)?;
        match nanoid_with(size, &alphabet) {
            Ok(s) => Ok(str_val(s)),
            Err(e) => Ok(nid_err(span, map_nanoid_err(e))),
        }
    } else {
        match nanoid_size(size) {
            Ok(s) => Ok(str_val(s)),
            Err(e) => Ok(nid_err(span, map_nanoid_err(e))),
        }
    }
}

// >>> len(nid.nanoid_bulk(3))
// 3
fn nid_nanoid_bulk(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nid_nanoid_bulk", span)?;
    let count = int_arg(args, 0, "nid_nanoid_bulk", span)?;
    if count < 0 {
        return Ok(nid_err(span, "count must be >= 0"));
    }
    let size = optional_int(args, 1, NANOID_DEFAULT_SIZE as i64) as usize;
    if size == 0 {
        return Ok(nid_err(span, "size must be > 0"));
    }
    let alphabet = optional_string(args, 2, NANOID_DEFAULT_ALPHABET);
    match nanoid_bulk(count as usize, size, &alphabet) {
        Ok(ids) => {
            let arr: Vec<ValueRef> = ids.into_iter().map(str_val).collect();
            Ok(Value::Array(arr).ref_cell())
        }
        Err(e) => Ok(nid_err(span, map_nanoid_err(e))),
    }
}

// ---------------------------------------------------------------------------
// Snowflake
// ---------------------------------------------------------------------------

// >>> type(nid.snowflake())
// "int"
fn nid_snowflake(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 2, "nid_snowflake", span)?;
    let worker = optional_int(args, 0, 0) as u16;
    let dc = optional_int(args, 1, 0) as u16;
    match SnowflakeGenerator::new(worker, dc) {
        Ok(gen) => match gen.next_id() {
            Ok(id) => Ok(int_val(id)),
            Err(e) => Ok(nid_err(span, map_snowflake_err(e))),
        },
        Err(e) => Ok(nid_err(span, map_snowflake_err(e))),
    }
}

// >>> type(nid.snowflake_maker().next())
// "int"
fn nid_snowflake_maker(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 3, "nid_snowflake_maker", span)?;
    let worker = optional_int(args, 0, 0) as u16;
    let dc = optional_int(args, 1, 0) as u16;
    let epoch = optional_int(args, 2, SNOWFLAKE_DEFAULT_EPOCH as i64) as u64;
    let gen = match SnowflakeGenerator::with_epoch(worker, dc, epoch) {
        Ok(g) => Arc::new(g),
        Err(e) => return Ok(nid_err(span, map_snowflake_err(e))),
    };
    let id = register(NidHandle::Snowflake(Arc::clone(&gen)));
    let mut methods = HashMap::new();
    methods.insert(
        "next".to_string(),
        Value::NativeFunction(Rc::new(nid_snowflake_maker_next)).ref_cell(),
    );
    methods.insert("epoch".to_string(), int_val(epoch as i64));
    methods.insert("worker_id".to_string(), int_val(worker as i64));
    methods.insert(
        "datacenter_id".to_string(),
        int_val(dc as i64),
    );
    Ok(handle_object(id, "snowflake_maker", methods))
}

fn nid_snowflake_maker_next(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "snowflake_maker.next", span)?;
    let id = handle_id_from_arg(args, 0, span, "snowflake_maker.next")?;
    match with_handle(id, span, |h| {
        if let NidHandle::Snowflake(gen) = h {
            gen.next_id()
        } else {
            Err(SnowflakeError::WorkerIdOutOfRange)
        }
    })? {
        Ok(Ok(n)) => Ok(int_val(n)),
        Ok(Err(e)) => Ok(nid_err(span, map_snowflake_err(e))),
        Err(e) => Ok(e),
    }
}

// >>> nid.snowflake_parse(nid.snowflake()).worker_id >= 0
// true
fn nid_snowflake_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nid_snowflake_parse", span)?;
    let raw = int_arg(args, 0, "nid_snowflake_parse", span)?;
    let epoch = optional_int(args, 1, SNOWFLAKE_DEFAULT_EPOCH as i64) as u64;
    let parts = snowflake_parse(raw, epoch);
    let mut map = HashMap::new();
    map.insert("timestamp".to_string(), int_val(parts.timestamp_ms as i64));
    map.insert(
        "datacenter_id".to_string(),
        int_val(parts.datacenter_id as i64),
    );
    map.insert("worker_id".to_string(), int_val(parts.worker_id as i64));
    map.insert("sequence".to_string(), int_val(parts.sequence as i64));
    Ok(Value::Object(map).ref_cell())
}

// ---------------------------------------------------------------------------
// Hashids
// ---------------------------------------------------------------------------

// >>> type(nid.hashids("salt").encode(byte_array[1, 2]))
// "string"
fn nid_hashids(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 3, "nid_hashids", span)?;
    let salt = optional_string(args, 0, "");
    let min_len = optional_int(args, 1, 0) as usize;
    let alphabet = optional_string(args, 2, HASHIDS_DEFAULT_ALPHABET);
    match Hashids::new(&salt, min_len, &alphabet) {
        Ok(enc) => {
            let id = register(NidHandle::Hashids(enc));
            let mut methods = HashMap::new();
            methods.insert(
                "encode".to_string(),
                Value::NativeFunction(Rc::new(nid_hashids_encode_method)).ref_cell(),
            );
            methods.insert(
                "decode".to_string(),
                Value::NativeFunction(Rc::new(nid_hashids_decode_method)).ref_cell(),
            );
            methods.insert(
                "encode_hex".to_string(),
                Value::NativeFunction(Rc::new(nid_hashids_encode_hex_method)).ref_cell(),
            );
            methods.insert(
                "decode_hex".to_string(),
                Value::NativeFunction(Rc::new(nid_hashids_decode_hex_method)).ref_cell(),
            );
            Ok(handle_object(id, "hashids", methods))
        }
        Err(e) => Ok(nid_err(span, map_hashids_err(e))),
    }
}

fn nid_hashids_encode_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() < 2 {
        return Err(RuntimeError::at(
            span,
            E3534_NID_ARITY,
            format!("hashids.encode() expects handle plus numbers, got {}", args.len()),
        ));
    }
    let handle_id = handle_id_from_arg(args, 0, span, "hashids.encode")?;
    let nums = match u64_array_from_args(args, 1, span, "hashids.encode")? {
        Ok(v) => v,
        Err(e) => return Ok(e),
    };
    match with_handle(handle_id, span, |h| {
        if let NidHandle::Hashids(enc) = h {
            enc.encode(&nums)
        } else {
            Err(HashidsError::EmptyInput)
        }
    })? {
        Ok(Ok(s)) => Ok(str_val(s)),
        Ok(Err(e)) => Ok(nid_err(span, map_hashids_err(e))),
        Err(e) => Ok(e),
    }
}

fn nid_hashids_decode_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 255, "hashids.decode", span)?;
    let handle_id = handle_id_from_arg(args, 0, span, "hashids.decode")?;
    let hash = string_arg(args, 1, "hashids.decode", span)?;
    match with_handle(handle_id, span, |h| {
        if let NidHandle::Hashids(enc) = h {
            enc.decode(&hash)
        } else {
            Err(HashidsError::InvalidHash)
        }
    })? {
        Ok(Ok(nums)) => {
            let arr: Vec<ValueRef> = nums.into_iter().map(|n| int_val(n as i64)).collect();
            Ok(Value::Array(arr).ref_cell())
        }
        Ok(Err(e)) => Ok(nid_err(span, map_hashids_err(e))),
        Err(e) => Ok(e),
    }
}

fn nid_hashids_encode_hex_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "hashids.encode_hex", span)?;
    let handle_id = handle_id_from_arg(args, 0, span, "hashids.encode_hex")?;
    let hex = string_arg(args, 1, "hashids.encode_hex", span)?;
    match with_handle(handle_id, span, |h| {
        if let NidHandle::Hashids(enc) = h {
            enc.encode_hex(&hex)
        } else {
            Err(HashidsError::EmptyInput)
        }
    })? {
        Ok(Ok(s)) => Ok(str_val(s)),
        Ok(Err(e)) => Ok(nid_err(span, map_hashids_err(e))),
        Err(e) => Ok(e),
    }
}

fn nid_hashids_decode_hex_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "hashids.decode_hex", span)?;
    let handle_id = handle_id_from_arg(args, 0, span, "hashids.decode_hex")?;
    let hash = string_arg(args, 1, "hashids.decode_hex", span)?;
    match with_handle(handle_id, span, |h| {
        if let NidHandle::Hashids(enc) = h {
            enc.decode_hex(&hash)
        } else {
            Err(HashidsError::InvalidHash)
        }
    })? {
        Ok(Ok(s)) => Ok(str_val(s)),
        Ok(Err(e)) => Ok(nid_err(span, map_hashids_err(e))),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! nid_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nid_fns![
    ("nid_uuid4", "uuid4", nid_uuid4),
    ("nid_uuid6", "uuid6", nid_uuid6),
    ("nid_uuid7", "uuid7", nid_uuid7),
    ("nid_uuid_parse", "uuid_parse", nid_uuid_parse),
    ("nid_uuid_is_valid", "uuid_is_valid", nid_uuid_is_valid),
    ("nid_uuid_version", "uuid_version", nid_uuid_version),
    ("nid_uuid_bytes", "uuid_bytes", nid_uuid_bytes),
    ("nid_uuid_from_bytes", "uuid_from_bytes", nid_uuid_from_bytes),
    ("nid_uuid_timestamp", "uuid_timestamp", nid_uuid_timestamp),
    ("nid_ulid", "ulid", nid_ulid),
    ("nid_ulid_parse", "ulid_parse", nid_ulid_parse),
    ("nid_ulid_is_valid", "ulid_is_valid", nid_ulid_is_valid),
    ("nid_ulid_timestamp", "ulid_timestamp", nid_ulid_timestamp),
    ("nid_ulid_maker", "ulid_maker", nid_ulid_maker),
    ("nid_nanoid", "nanoid", nid_nanoid),
    ("nid_nanoid_bulk", "nanoid_bulk", nid_nanoid_bulk),
    ("nid_snowflake", "snowflake", nid_snowflake),
    ("nid_snowflake_maker", "snowflake_maker", nid_snowflake_maker),
    ("nid_snowflake_parse", "snowflake_parse", nid_snowflake_parse),
    ("nid_hashids", "hashids", nid_hashids),
];

pub const MODULE_NAME: &str = "nid";
pub const MODULE_PATHS: &[&str] = &["nid", "std/nid"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    map.insert(
        "NANOID_ALPHABET".to_string(),
        str_val(NANOID_DEFAULT_ALPHABET),
    );
    map.insert(
        "NANOID_SIZE".to_string(),
        int_val(NANOID_DEFAULT_SIZE as i64),
    );
    map.insert(
        "SNOWFLAKE_EPOCH".to_string(),
        int_val(SNOWFLAKE_DEFAULT_EPOCH as i64),
    );
    map.insert(
        "HASHIDS_ALPHABET".to_string(),
        str_val(HASHIDS_DEFAULT_ALPHABET),
    );
    map.insert(
        "MAX_WORKER_ID".to_string(),
        int_val(MAX_WORKER_ID as i64),
    );
    map.insert(
        "MAX_DATACENTER_ID".to_string(),
        int_val(MAX_DATACENTER_ID as i64),
    );
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
    fn uuid4_valid() {
        let u = nid_uuid4(&[], span()).unwrap();
        match &*u.borrow() {
            Value::String(s) => assert!(uuid_is_valid(s)),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn ulid_len() {
        let u = nid_ulid(&[], span()).unwrap();
        match &*u.borrow() {
            Value::String(s) => assert_eq!(s.len(), 26),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn hashids_roundtrip() {
        let enc = nid_hashids(&[Value::String("salt".into()).ref_cell()], span()).unwrap();
        let handle = match &*enc.borrow() {
            Value::Object(map) => map["id"].clone(),
            other => panic!("expected object, got {other:?}"),
        };
        let hash = nid_hashids_encode_method(
            &[handle.clone(), Value::Int(42).ref_cell(), Value::Int(7).ref_cell()],
            span(),
        )
        .unwrap();
        let nums = nid_hashids_decode_method(&[handle, hash], span()).unwrap();
        match &*nums.borrow() {
            Value::Array(items) => {
                assert_eq!(items.len(), 2);
                assert!(matches!(&*items[0].borrow(), Value::Int(42)));
                assert!(matches!(&*items[1].borrow(), Value::Int(7)));
            }
            other => panic!("expected array, got {other:?}"),
        }
    }
}
