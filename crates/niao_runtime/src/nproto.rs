//! Native nproto standard library — Protocol Buffers compile, dynamic
//! encode/decode, JSON mapping, wire introspection, and codegen (~protobuf).
//!
//! Import with `import "nproto"` (or `import "std/nproto"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_bignum::BigInt;
use niao_errors::codes;
use niao_proto::{
    codegen, compile_files, compile_source, decode_raw, decode_varint, dynamic_to_niao,
    encode_tag, encode_varint, load_descriptor_set, niao_to_dynamic, valid_descriptor_set,
    CodegenOptions, FieldInfo, MessageInfo, NiaoFieldValue, OneofInfo, ProtoError, ProtoMessage,
    ProtoMessageRef, ProtoSchema, RawValue, MAX_BYTES,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E4340: u32 = codes::E4340_NPROTO_ARITY;
const E4341: u32 = codes::E4341_NPROTO_ERROR;
const E4342: u32 = codes::E4342_NPROTO_TYPE;
const E4343: u32 = codes::E4343_NPROTO_PARSE;
const E4344: u32 = codes::E4344_NPROTO_INVALID_HANDLE;

thread_local! {
    static SCHEMA_STORE: RefCell<HashMap<i64, ProtoSchema>> = RefCell::new(HashMap::new());
    static MESSAGE_STORE: RefCell<HashMap<i64, ProtoMessage>> = RefCell::new(HashMap::new());
    static NEXT_SCHEMA_ID: RefCell<i64> = const { RefCell::new(1) };
    static NEXT_MESSAGE_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_schema(schema: ProtoSchema) -> i64 {
    let id = NEXT_SCHEMA_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    SCHEMA_STORE.with(|m| m.borrow_mut().insert(id, schema));
    id
}

fn alloc_message(msg: ProtoMessage) -> i64 {
    let id = NEXT_MESSAGE_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    MESSAGE_STORE.with(|m| m.borrow_mut().insert(id, msg));
    id
}

fn invalid_schema_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E4344,
        "nproto_error",
        format!("invalid or closed nproto schema handle {id}"),
        span,
    )
}

fn invalid_message_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E4344,
        "nproto_error",
        format!("invalid or closed nproto message handle {id}"),
        span,
    )
}

fn with_schema<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&ProtoSchema) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    SCHEMA_STORE.with(|m| {
        match m.borrow().get(&id) {
            Some(s) => Ok(Ok(f(s))),
            None => Ok(Err(invalid_schema_handle(span, id))),
        }
    })
}

fn with_message<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&ProtoMessage) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    MESSAGE_STORE.with(|m| {
        match m.borrow().get(&id) {
            Some(msg) => Ok(Ok(f(msg))),
            None => Ok(Err(invalid_message_handle(span, id))),
        }
    })
}

fn with_message_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut ProtoMessage) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    MESSAGE_STORE.with(|m| {
        match m.borrow_mut().get_mut(&id) {
            Some(msg) => Ok(Ok(f(msg))),
            None => Ok(Err(invalid_message_handle(span, id))),
        }
    })
}

fn get_schema(id: i64, span: Span) -> NiaoResult<ProtoSchema> {
    match with_schema(id, span, |s| s.clone())? {
        Ok(s) => Ok(s),
        Err(v) => {
            let msg = error_message(&v);
            Err(RuntimeError::at(span, E4341, msg))
        }
    }
}

fn get_message(id: i64, span: Span) -> NiaoResult<ProtoMessage> {
    match with_message(id, span, |m| m.clone())? {
        Ok(m) => Ok(m),
        Err(v) => {
            let msg = error_message(&v);
            Err(RuntimeError::at(span, E4341, msg))
        }
    }
}

fn error_message(v: &ValueRef) -> String {
    match &*v.borrow() {
        Value::Object(m) => m
            .get("message")
            .map(|x| match &*x.borrow() {
                Value::String(s) => s.clone(),
                _ => "nproto error".into(),
            })
            .unwrap_or_else(|| "nproto error".into()),
        _ => "nproto error".into(),
    }
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4342, msg.into())
}

fn nproto_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4341, "nproto_error", msg.into(), span)
}

fn map_proto_err(span: Span, err: ProtoError) -> ValueRef {
    let code = match &err {
        ProtoError::Compile(_) | ProtoError::Parse(_) => E4343,
        ProtoError::Type(_) => E4342,
        _ => E4341,
    };
    error_value(code, "nproto_error", err.to_string(), span)
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4340,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4340,
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

fn schema_handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a schema handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn message_handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a message handle as argument {}, got {}",
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

fn optional_int(args: &[ValueRef], idx: usize, default: i64) -> i64 {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Int(n) => *n,
        _ => default,
    }
}

fn string_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: &str) -> String {
    let Some(map) = map else {
        return default.to_string();
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => s,
        _ => default.to_string(),
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

fn string_array_from_map(
    map: Option<&HashMap<String, ValueRef>>,
    key: &str,
) -> Vec<String> {
    let Some(map) = map else {
        return Vec::new();
    };
    let Some(v) = map.get(key) else {
        return Vec::new();
    };
    match &*v.borrow() {
        Value::Array(items) => items
            .iter()
            .filter_map(|item| match &*item.borrow() {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn string_array_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Vec<String>> {
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
                                "{name}() array item {} must be string, got {}",
                                i + 1,
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
                "{name}() expects an array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
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

fn compile_opts(map: Option<&HashMap<String, ValueRef>>) -> (String, Vec<String>) {
    (
        string_field(map, "filename", "input.proto"),
        string_array_from_map(map, "include_paths"),
    )
}

fn codegen_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> CodegenOptions {
    let module_name = map.and_then(|m| {
        m.get("module_name")
            .and_then(|v| match &*v.borrow() {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
    });
    CodegenOptions {
        module_name,
        include_helpers: bool_field(map, "include_helpers", false),
    }
}

fn ok_schema(schema: ProtoSchema) -> NiaoResult<ValueRef> {
    Ok(Value::Int(alloc_schema(schema)).ref_cell())
}

fn ok_message(msg: ProtoMessage) -> NiaoResult<ValueRef> {
    Ok(Value::Int(alloc_message(msg)).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn str_val(s: impl Into<String>) -> NiaoResult<ValueRef> {
    Ok(Value::String(s.into()).ref_cell())
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

fn bytes_val(bytes: Vec<u8>) -> NiaoResult<ValueRef> {
    Ok(Value::ByteArray(bytes).ref_cell())
}

fn strings_to_array(items: &[String]) -> ValueRef {
    Value::Array(
        items
            .iter()
            .map(|s| Value::String(s.clone()).ref_cell())
            .collect(),
    )
    .ref_cell()
}

fn optional_string_to_value(s: Option<String>) -> ValueRef {
    match s {
        Some(v) => Value::String(v).ref_cell(),
        None => Value::Nil.ref_cell(),
    }
}

// ---------------------------------------------------------------------------
// Niao Value ↔ NiaoFieldValue bridge
// ---------------------------------------------------------------------------

fn field_to_niao(fv: NiaoFieldValue) -> Value {
    match fv {
        NiaoFieldValue::Null => Value::Nil,
        NiaoFieldValue::Bool(b) => Value::Bool(b),
        NiaoFieldValue::Int(n) => Value::Int(n),
        NiaoFieldValue::Float(f) => Value::Float(f),
        NiaoFieldValue::String(s) => Value::String(s),
        NiaoFieldValue::Bytes(b) => Value::ByteArray(b),
        NiaoFieldValue::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|i| field_to_niao(i).ref_cell())
                .collect(),
        ),
        NiaoFieldValue::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k, field_to_niao(v).ref_cell());
            }
            Value::Object(out)
        }
        NiaoFieldValue::Message(m) => {
            let mut map = HashMap::new();
            map.insert("__type".into(), Value::String(m.full_name).ref_cell());
            for (k, v) in m.fields {
                map.insert(k, field_to_niao(v).ref_cell());
            }
            Value::Object(map)
        }
    }
}

fn niao_to_field_value(v: &Value, span: Span) -> NiaoResult<NiaoFieldValue> {
    match v {
        Value::Nil => Ok(NiaoFieldValue::Null),
        Value::Bool(b) => Ok(NiaoFieldValue::Bool(*b)),
        Value::Int(n) => Ok(NiaoFieldValue::Int(*n)),
        Value::Float(f) => Ok(NiaoFieldValue::Float(*f)),
        Value::String(s) => Ok(NiaoFieldValue::String(s.clone())),
        Value::ByteArray(b) => Ok(NiaoFieldValue::Bytes(b.clone())),
        Value::BigInt(n) => {
            if let Some(i) = n.to_i64() {
                Ok(NiaoFieldValue::Int(i))
            } else {
                Err(type_err(
                    span,
                    "nproto: bigint field value out of int64 range",
                ))
            }
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(niao_to_field_value(&item.borrow(), span)?);
            }
            Ok(NiaoFieldValue::Array(out))
        }
        Value::Object(map) => {
            let mut fields = HashMap::with_capacity(map.len());
            let mut full_name = None;
            for (k, slot) in map {
                if k == "__type" {
                    if let Value::String(s) = &*slot.borrow() {
                        full_name = Some(s.clone());
                    }
                    continue;
                }
                fields.insert(k.clone(), niao_to_field_value(&slot.borrow(), span)?);
            }
            if let Some(name) = full_name {
                return Ok(NiaoFieldValue::Message(ProtoMessageRef {
                    full_name: name,
                    fields,
                }));
            }
            Ok(NiaoFieldValue::Object(fields))
        }
        other => Err(type_err(
            span,
            format!(
                "nproto: cannot convert {} to protobuf field value",
                other.type_name()
            ),
        )),
    }
}

fn niao_fields_map(
    obj: Option<&HashMap<String, ValueRef>>,
    span: Span,
) -> NiaoResult<HashMap<String, NiaoFieldValue>> {
    let Some(obj) = obj else {
        return Ok(HashMap::new());
    };
    let mut out = HashMap::with_capacity(obj.len());
    for (k, v) in obj {
        out.insert(k.clone(), niao_to_field_value(&v.borrow(), span)?);
    }
    Ok(out)
}

fn field_info_to_object(f: &FieldInfo) -> Value {
    let mut map = HashMap::new();
    map.insert("name".into(), Value::String(f.name.clone()).ref_cell());
    map.insert("number".into(), Value::Int(f.number as i64).ref_cell());
    map.insert("kind".into(), Value::String(f.kind.clone()).ref_cell());
    map.insert("label".into(), Value::String(f.label.clone()).ref_cell());
    map.insert("json_name".into(), Value::String(f.json_name.clone()).ref_cell());
    map.insert(
        "message_type".into(),
        optional_string_to_value(f.message_type.clone()),
    );
    map.insert("enum_type".into(), optional_string_to_value(f.enum_type.clone()));
    map.insert(
        "map_key_type".into(),
        optional_string_to_value(f.map_key_type.clone()),
    );
    map.insert(
        "map_value_type".into(),
        optional_string_to_value(f.map_value_type.clone()),
    );
    map.insert(
        "default_value".into(),
        optional_string_to_value(f.default_value.clone()),
    );
    map.insert("oneof".into(), optional_string_to_value(f.oneof.clone()));
    Value::Object(map)
}

fn oneof_info_to_object(o: &OneofInfo) -> Value {
    let mut map = HashMap::new();
    map.insert("name".into(), Value::String(o.name.clone()).ref_cell());
    map.insert(
        "fields".into(),
        strings_to_array(&o.fields),
    );
    Value::Object(map)
}

fn message_info_to_object(info: &MessageInfo) -> Value {
    let mut map = HashMap::new();
    map.insert("name".into(), Value::String(info.name.clone()).ref_cell());
    map.insert(
        "full_name".into(),
        Value::String(info.full_name.clone()).ref_cell(),
    );
    map.insert(
        "fields".into(),
        Value::Array(
            info.fields
                .iter()
                .map(|f| field_info_to_object(f).ref_cell())
                .collect(),
        )
        .ref_cell(),
    );
    map.insert(
        "oneofs".into(),
        Value::Array(
            info.oneofs
                .iter()
                .map(|o| oneof_info_to_object(o).ref_cell())
                .collect(),
        )
        .ref_cell(),
    );
    Value::Object(map)
}

fn raw_value_to_niao(v: &RawValue) -> ValueRef {
    match v {
        RawValue::Varint(n) => Value::Int(*n as i64).ref_cell(),
        RawValue::Fixed32(n) => Value::Int(*n as i64).ref_cell(),
        RawValue::Fixed64(n) => {
            if *n <= i64::MAX as u64 {
                Value::Int(*n as i64).ref_cell()
            } else {
                Value::BigInt(BigInt::from(*n)).ref_cell()
            }
        }
        RawValue::LengthDelimited(b) => Value::ByteArray(b.clone()).ref_cell(),
    }
}

fn raw_field_to_object(field: &niao_proto::RawField) -> Value {
    let mut map = HashMap::new();
    map.insert(
        "field_number".into(),
        Value::Int(field.field_number as i64).ref_cell(),
    );
    map.insert("wire_type".into(), Value::Int(field.wire_type as i64).ref_cell());
    map.insert(
        "wire_name".into(),
        Value::String(field.wire_name.clone()).ref_cell(),
    );
    map.insert("value".into(), raw_value_to_niao(&field.value));
    Value::Object(map)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> type(nproto.compile('syntax = "proto3"; message M { string x = 1; }'))
// => "int"
fn nproto_compile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproto_compile", span)?;
    let source = string_arg(args, 0, "nproto_compile", span)?;
    if source.len() > MAX_BYTES {
        return Ok(nproto_err(
            span,
            format!("source size {} exceeds limit {MAX_BYTES}", source.len()),
        ));
    }
    let opts = optional_object_arg(args, 1);
    let (filename, includes) = compile_opts(opts.as_ref());
    let include_refs: Vec<&str> = includes.iter().map(|s| s.as_str()).collect();
    match compile_source(&filename, &source, &include_refs) {
        Ok(schema) => ok_schema(schema),
        Err(e) => Ok(map_proto_err(span, e)),
    }
}

// >>> type(nproto.compile_file("echo.proto"))
// => "int"
fn nproto_compile_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproto_compile_file", span)?;
    let path = string_arg(args, 0, "nproto_compile_file", span)?;
    let includes = string_array_from_map(optional_object_arg(args, 1).as_ref(), "include_paths");
    let include_refs: Vec<&str> = includes.iter().map(|s| s.as_str()).collect();
    match compile_files(&[path.as_str()], &include_refs) {
        Ok(schema) => ok_schema(schema),
        Err(e) => Ok(map_proto_err(span, e)),
    }
}

// >>> len(nproto.compile_files(["a.proto", "b.proto"]))
// => 1
fn nproto_compile_files(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproto_compile_files", span)?;
    let files = string_array_arg(args, 0, "nproto_compile_files", span)?;
    let file_refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
    let includes = string_array_from_map(optional_object_arg(args, 1).as_ref(), "include_paths");
    let include_refs: Vec<&str> = includes.iter().map(|s| s.as_str()).collect();
    match compile_files(&file_refs, &include_refs) {
        Ok(schema) => ok_schema(schema),
        Err(e) => Ok(map_proto_err(span, e)),
    }
}

// >>> type(nproto.load_descriptor_set(bytes))
// => "int"
fn nproto_load_descriptor_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_load_descriptor_set", span)?;
    let bytes = bytes_arg(args, 0, "nproto_load_descriptor_set", span)?;
    if bytes.len() > MAX_BYTES {
        return Ok(nproto_err(
            span,
            format!("descriptor set size {} exceeds limit {MAX_BYTES}", bytes.len()),
        ));
    }
    match load_descriptor_set(&bytes) {
        Ok(schema) => ok_schema(schema),
        Err(e) => Ok(map_proto_err(span, e)),
    }
}

// >>> len(nproto.save_descriptor_set(schema))
// => 1
fn nproto_save_descriptor_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_save_descriptor_set", span)?;
    let id = schema_handle_arg(args, 0, "nproto_save_descriptor_set", span)?;
    match with_schema(id, span, |schema| schema.encode_descriptor_set())? {
        Ok(Ok(bytes)) => bytes_val(bytes),
        Ok(Err(e)) => Ok(map_proto_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> len(nproto.message_names(schema))
// => 1
fn nproto_message_names(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_message_names", span)?;
    let id = schema_handle_arg(args, 0, "nproto_message_names", span)?;
    match with_schema(id, span, |schema| schema.message_names())? {
        Ok(names) => Ok(strings_to_array(&names)),
        Err(v) => Ok(v),
    }
}

// >>> type(nproto.enum_names(schema))
// => "array"
fn nproto_enum_names(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_enum_names", span)?;
    let id = schema_handle_arg(args, 0, "nproto_enum_names", span)?;
    match with_schema(id, span, |schema| schema.enum_names())? {
        Ok(names) => Ok(strings_to_array(&names)),
        Err(v) => Ok(v),
    }
}

// >>> nproto.describe(schema, "test.Echo").fields[0].name
// => "text"
fn nproto_describe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nproto_describe", span)?;
    let id = schema_handle_arg(args, 0, "nproto_describe", span)?;
    let message_name = string_arg(args, 1, "nproto_describe", span)?;
    match with_schema(id, span, |schema| schema.describe_message(&message_name))? {
        Ok(Ok(info)) => Ok(message_info_to_object(&info).ref_cell()),
        Ok(Err(e)) => Ok(map_proto_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> nproto.get(nproto.new_message(schema, "test.Echo", {text: "hi"}), "text")
// => "hi"
fn nproto_new_message(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nproto_new_message", span)?;
    let schema_id = schema_handle_arg(args, 0, "nproto_new_message", span)?;
    let message_name = string_arg(args, 1, "nproto_new_message", span)?;
    let fields = niao_fields_map(optional_object_arg(args, 2).as_ref(), span)?;
    let schema = get_schema(schema_id, span)?;
    match ProtoMessage::new(&schema, &message_name) {
        Ok(mut msg) => {
            if !fields.is_empty() {
                if let Err(e) = msg.apply_niao_fields(&fields) {
                    return Ok(map_proto_err(span, e));
                }
            }
            ok_message(msg)
        }
        Err(e) => Ok(map_proto_err(span, e)),
    }
}

// >>> nproto.type_name(nproto.decode(schema, "test.Echo", bytes))
// => "test.Echo"
fn nproto_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nproto_decode", span)?;
    let schema_id = schema_handle_arg(args, 0, "nproto_decode", span)?;
    let message_name = string_arg(args, 1, "nproto_decode", span)?;
    let bytes = bytes_arg(args, 2, "nproto_decode", span)?;
    if bytes.len() > MAX_BYTES {
        return Ok(nproto_err(
            span,
            format!("input size {} exceeds limit {MAX_BYTES}", bytes.len()),
        ));
    }
    let schema = get_schema(schema_id, span)?;
    match ProtoMessage::decode(&schema, &message_name, &bytes) {
        Ok(msg) => ok_message(msg),
        Err(e) => Ok(map_proto_err(span, e)),
    }
}

// >>> nproto.get(nproto.from_json(schema, "test.Echo", '{"text":"ok"}'), "text")
// => "ok"
fn nproto_from_json(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nproto_from_json", span)?;
    let schema_id = schema_handle_arg(args, 0, "nproto_from_json", span)?;
    let message_name = string_arg(args, 1, "nproto_from_json", span)?;
    let json_text = string_arg(args, 2, "nproto_from_json", span)?;
    let schema = get_schema(schema_id, span)?;
    match ProtoMessage::from_json(&schema, &message_name, &json_text) {
        Ok(msg) => ok_message(msg),
        Err(e) => Ok(map_proto_err(span, e)),
    }
}

// >>> nproto.get(msg, "text")
// => "hi"
fn nproto_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nproto_get", span)?;
    let msg_id = message_handle_arg(args, 0, "nproto_get", span)?;
    let field = string_arg(args, 1, "nproto_get", span)?;
    match with_message(msg_id, span, |msg| -> Result<ValueRef, ProtoError> {
        if !msg.has_field(&field)? {
            return Ok(Value::Nil.ref_cell());
        }
        let dynamic = msg.get_dynamic(&field)?;
        let fv = dynamic_to_niao(&dynamic)?;
        Ok(field_to_niao(fv).ref_cell())
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(map_proto_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> nproto.get(nproto.set(msg, "text", "bye"), "text")
// => "bye"
fn nproto_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nproto_set", span)?;
    let msg_id = message_handle_arg(args, 0, "nproto_set", span)?;
    let field = string_arg(args, 1, "nproto_set", span)?;
    let value = args[2].borrow().clone();
    if matches!(value, Value::Nil) {
        return match with_message_mut(msg_id, span, |msg| {
            msg.set_field_json(&field, &serde_json::Value::Null)
        })? {
            Ok(Ok(())) => bool_val(true),
            Ok(Err(e)) => Ok(map_proto_err(span, e)),
            Err(v) => Ok(v),
        };
    }
    let fv = niao_to_field_value(&value, span)?;
    match with_message_mut(msg_id, span, |msg| {
        let fd = msg
            .descriptor()
            .get_field_by_name(&field)
            .ok_or_else(|| ProtoError::Schema(format!("unknown field: {field}")))?;
        let dynamic = niao_to_dynamic(&fd, &fv)?;
        msg.set_dynamic(&field, dynamic)
    })? {
        Ok(Ok(())) => bool_val(true),
        Ok(Err(e)) => Ok(map_proto_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> nproto.has(msg, "text")
// => true
fn nproto_has(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nproto_has", span)?;
    let msg_id = message_handle_arg(args, 0, "nproto_has", span)?;
    let field = string_arg(args, 1, "nproto_has", span)?;
    match with_message(msg_id, span, |msg| msg.has_field(&field))? {
        Ok(Ok(b)) => bool_val(b),
        Ok(Err(e)) => Ok(map_proto_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> nproto.fields(msg).text
// => "hi"
fn nproto_fields(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_fields", span)?;
    let msg_id = message_handle_arg(args, 0, "nproto_fields", span)?;
    match with_message(msg_id, span, |msg| msg.to_niao_map())? {
        Ok(Ok(map)) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k, field_to_niao(v).ref_cell());
            }
            Ok(Value::Object(out).ref_cell())
        }
        Ok(Err(e)) => Ok(map_proto_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> len(nproto.encode(msg))
// => 1
fn nproto_encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_encode", span)?;
    let msg_id = message_handle_arg(args, 0, "nproto_encode", span)?;
    match with_message(msg_id, span, |msg| msg.encode())? {
        Ok(Ok(bytes)) => bytes_val(bytes),
        Ok(Err(e)) => Ok(map_proto_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> nproto.get(nproto.merge(dst, src), "text")
// => "merged"
fn nproto_merge(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nproto_merge", span)?;
    let dst_id = message_handle_arg(args, 0, "nproto_merge", span)?;
    let src_id = message_handle_arg(args, 1, "nproto_merge", span)?;
    let src = get_message(src_id, span)?;
    match with_message_mut(dst_id, span, |dst| dst.merge(&src))? {
        Ok(Ok(())) => bool_val(true),
        Ok(Err(e)) => Ok(map_proto_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> nproto.type_name(nproto.clone(msg))
// => "test.Echo"
fn nproto_clone(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_clone", span)?;
    let msg_id = message_handle_arg(args, 0, "nproto_clone", span)?;
    match with_message(msg_id, span, |msg| msg.clone_msg())? {
        Ok(Ok(cloned)) => ok_message(cloned),
        Ok(Err(e)) => Ok(map_proto_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> nproto.has(nproto.clear(msg), "text")
// => false
fn nproto_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_clear", span)?;
    let msg_id = message_handle_arg(args, 0, "nproto_clear", span)?;
    match with_message_mut(msg_id, span, |msg| {
        msg.clear();
    })? {
        Ok(()) => bool_val(true),
        Err(v) => Ok(v),
    }
}

// >>> nproto.to_json(msg)
// => "{\"text\":\"hi\"}"
fn nproto_to_json(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproto_to_json", span)?;
    let msg_id = message_handle_arg(args, 0, "nproto_to_json", span)?;
    let pretty = optional_bool(args, 1, false);
    match with_message(msg_id, span, |msg| msg.to_json(pretty))? {
        Ok(Ok(text)) => str_val(text),
        Ok(Err(e)) => Ok(map_proto_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> nproto.type_name(msg)
// => "test.Echo"
fn nproto_type_name(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_type_name", span)?;
    let msg_id = message_handle_arg(args, 0, "nproto_type_name", span)?;
    match with_message(msg_id, span, |msg| msg.full_name())? {
        Ok(name) => str_val(name),
        Err(v) => Ok(v),
    }
}

// >>> nproto.close_schema(schema)
// => true
fn nproto_close_schema(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_close_schema", span)?;
    let id = schema_handle_arg(args, 0, "nproto_close_schema", span)?;
    let removed = SCHEMA_STORE.with(|m| m.borrow_mut().remove(&id).is_some());
    bool_val(removed)
}

// >>> nproto.close_message(msg)
// => true
fn nproto_close_message(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_close_message", span)?;
    let id = message_handle_arg(args, 0, "nproto_close_message", span)?;
    let removed = MESSAGE_STORE.with(|m| m.borrow_mut().remove(&id).is_some());
    bool_val(removed)
}

// >>> len(nproto.codegen(schema))
// => 1
fn nproto_codegen(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproto_codegen", span)?;
    let id = schema_handle_arg(args, 0, "nproto_codegen", span)?;
    let opts = codegen_opts_from_map(optional_object_arg(args, 1).as_ref());
    match with_schema(id, span, |schema| codegen(schema, &opts))? {
        Ok(Ok(text)) => str_val(text),
        Ok(Err(e)) => Ok(map_proto_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> len(nproto.decode_raw(bytes))
// => 2
fn nproto_decode_raw(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_decode_raw", span)?;
    let bytes = bytes_arg(args, 0, "nproto_decode_raw", span)?;
    if bytes.len() > MAX_BYTES {
        return Ok(nproto_err(
            span,
            format!("input size {} exceeds limit {MAX_BYTES}", bytes.len()),
        ));
    }
    match decode_raw(&bytes) {
        Ok(fields) => Ok(Value::Array(
            fields
                .iter()
                .map(|f| raw_field_to_object(f).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_proto_err(span, e)),
    }
}

// >>> len(nproto.encode_tag(3, 0))
// => 1
fn nproto_encode_tag(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nproto_encode_tag", span)?;
    let field_num = match &*args[0].borrow() {
        Value::Int(n) if *n >= 0 => *n as u32,
        other => {
            return Err(type_err(
                span,
                format!(
                    "nproto_encode_tag() expects non-negative int as argument 1, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    let wire_type = match &*args[1].borrow() {
        Value::Int(n) if (0..=5).contains(n) => *n as u8,
        other => {
            return Err(type_err(
                span,
                format!(
                    "nproto_encode_tag() expects wire type 0..=5 as argument 2, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    bytes_val(encode_tag(field_num, wire_type))
}

// >>> len(nproto.encode_varint(300))
// => 1
fn nproto_encode_varint(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_encode_varint", span)?;
    let n = match &*args[0].borrow() {
        Value::Int(v) if *v >= 0 => *v as u64,
        other => {
            return Err(type_err(
                span,
                format!(
                    "nproto_encode_varint() expects non-negative int, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    bytes_val(encode_varint(n))
}

// >>> nproto.decode_varint(bytes).value
// => 150
fn nproto_decode_varint(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproto_decode_varint", span)?;
    let bytes = bytes_arg(args, 0, "nproto_decode_varint", span)?;
    let offset = optional_int(args, 1, 0);
    if offset < 0 {
        return Err(type_err(span, "nproto_decode_varint() offset must be >= 0"));
    }
    let offset = offset as usize;
    match decode_varint(&bytes, offset) {
        Ok((value, next)) => {
            let mut map = HashMap::new();
            map.insert(
                "value".into(),
                if value <= i64::MAX as u64 {
                    Value::Int(value as i64).ref_cell()
                } else {
                    Value::BigInt(BigInt::from(value)).ref_cell()
                },
            );
            map.insert("offset".into(), Value::Int(next as i64).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_proto_err(span, e)),
    }
}

// >>> nproto.valid_descriptor_set(bytes)
// => true
fn nproto_valid_descriptor_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproto_valid_descriptor_set", span)?;
    let bytes = bytes_arg(args, 0, "nproto_valid_descriptor_set", span)?;
    bool_val(valid_descriptor_set(&bytes))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nproto_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nproto_fns![
    ("nproto_compile", "compile", nproto_compile),
    ("nproto_compile_file", "compile_file", nproto_compile_file),
    ("nproto_compile_files", "compile_files", nproto_compile_files),
    ("nproto_load_descriptor_set", "load_descriptor_set", nproto_load_descriptor_set),
    ("nproto_save_descriptor_set", "save_descriptor_set", nproto_save_descriptor_set),
    ("nproto_message_names", "message_names", nproto_message_names),
    ("nproto_enum_names", "enum_names", nproto_enum_names),
    ("nproto_describe", "describe", nproto_describe),
    ("nproto_new_message", "new_message", nproto_new_message),
    ("nproto_decode", "decode", nproto_decode),
    ("nproto_from_json", "from_json", nproto_from_json),
    ("nproto_get", "get", nproto_get),
    ("nproto_set", "set", nproto_set),
    ("nproto_has", "has", nproto_has),
    ("nproto_fields", "fields", nproto_fields),
    ("nproto_encode", "encode", nproto_encode),
    ("nproto_merge", "merge", nproto_merge),
    ("nproto_clone", "clone", nproto_clone),
    ("nproto_clear", "clear", nproto_clear),
    ("nproto_to_json", "to_json", nproto_to_json),
    ("nproto_type_name", "type_name", nproto_type_name),
    ("nproto_close_schema", "close_schema", nproto_close_schema),
    ("nproto_close_message", "close_message", nproto_close_message),
    ("nproto_codegen", "codegen", nproto_codegen),
    ("nproto_decode_raw", "decode_raw", nproto_decode_raw),
    ("nproto_encode_tag", "encode_tag", nproto_encode_tag),
    ("nproto_encode_varint", "encode_varint", nproto_encode_varint),
    ("nproto_decode_varint", "decode_varint", nproto_decode_varint),
    ("nproto_valid_descriptor_set", "valid_descriptor_set", nproto_valid_descriptor_set),
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

pub const MODULE_NAME: &str = "nproto";
pub const MODULE_PATHS: &[&str] = &["nproto", "std/nproto"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    const ECHO_PROTO: &str = r#"
syntax = "proto3";
package test;

message Echo {
  string text = 1;
  int32 n = 2;
}
"#;

    fn span() -> Span {
        Span::dummy()
    }

    fn compile_echo() -> i64 {
        let args = [Value::String(ECHO_PROTO.to_string()).ref_cell()];
        match nproto_compile(&args, span()) {
            Ok(v) => match &*v.borrow() {
                Value::Int(id) => *id,
                other => panic!("expected schema handle, got {other:?}"),
            },
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn compile_and_new_message() {
        let schema = compile_echo();
        let mut fields = HashMap::new();
        fields.insert("text".into(), Value::String("hello".into()).ref_cell());
        fields.insert("n".into(), Value::Int(42).ref_cell());
        let args = [
            Value::Int(schema).ref_cell(),
            Value::String("test.Echo".into()).ref_cell(),
            Value::Object(fields).ref_cell(),
        ];
        let msg = nproto_new_message(&args, span()).unwrap();
        let get_args = [msg, Value::String("text".into()).ref_cell()];
        let text = nproto_get(&get_args, span()).unwrap();
        match &*text.borrow() {
            Value::String(s) => assert_eq!(s, "hello"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_roundtrip() {
        let schema = compile_echo();
        let mut fields = HashMap::new();
        fields.insert("text".into(), Value::String("roundtrip".into()).ref_cell());
        let new_args = [
            Value::Int(schema).ref_cell(),
            Value::String("test.Echo".into()).ref_cell(),
            Value::Object(fields).ref_cell(),
        ];
        let msg = nproto_new_message(&new_args, span()).unwrap();
        let encoded = nproto_encode(&[msg], span()).unwrap();
        let decode_args = [
            Value::Int(schema).ref_cell(),
            Value::String("test.Echo".into()).ref_cell(),
            encoded,
        ];
        let decoded = nproto_decode(&decode_args, span()).unwrap();
        let get_args = [decoded, Value::String("text".into()).ref_cell()];
        let text = nproto_get(&get_args, span()).unwrap();
        match &*text.borrow() {
            Value::String(s) => assert_eq!(s, "roundtrip"),
            other => panic!("expected string, got {other:?}"),
        }
    }
}
