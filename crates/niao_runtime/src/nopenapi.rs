//! Native nopenapi standard library — OpenAPI 3 spec generation from ahiru
//! routes + typed client stub generation (~fastapi openapi, openapi-gen subset).
//!
//! Import with `import "nopenapi"` (or `import "std/nopenapi"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_openapi::{
    client_stub_str, from_ahiru, from_routes, infer_schema, is_valid, merge, normalize_path,
    operation, operation_id, parallel_client_stubs, parallel_validate, param, path_params,
    request_body, response, schema_array, schema_boolean, schema_integer, schema_number,
    schema_object, schema_ref, schema_string, validate, OpenApiDoc, OpenApiError,
};
use niao_parallel::available_threads;
use serde_json::{Map, Value as JsonValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

const E4120: u32 = codes::E4120_NOPENAPI_ARITY;
const E4121: u32 = codes::E4121_NOPENAPI_ERROR;
const E4122: u32 = codes::E4122_NOPENAPI_TYPE;
const E4123: u32 = codes::E4123_NOPENAPI_PARSE;
const E4124: u32 = codes::E4124_NOPENAPI_INVALID_HANDLE;

thread_local! {
    static DOCS: RefCell<HashMap<i64, OpenApiDoc>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn new_id() -> i64 {
    NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4122, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4120,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4120,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nopenapi_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4121, "nopenapi_error", msg.into(), span)
}

fn parse_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4123, "nopenapi_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E4124,
        "nopenapi_error",
        format!("invalid or closed nopenapi handle {id}"),
        span,
    )
}

fn map_err(span: Span, e: OpenApiError) -> ValueRef {
    nopenapi_err(span, e.message())
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

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a positive handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn bool_arg(args: &[ValueRef], idx: usize, default: bool) -> bool {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        Value::Nil => default,
        _ => default,
    }
}

fn to_json(v: &Value, span: Span) -> NiaoResult<JsonValue> {
    match v {
        Value::Nil => Ok(JsonValue::Null),
        Value::Bool(b) => Ok(JsonValue::Bool(*b)),
        Value::Int(n) => Ok(JsonValue::Number((*n).into())),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(JsonValue::Number)
            .ok_or_else(|| type_err(span, format!("non-finite float {f}"))),
        Value::String(s) => Ok(JsonValue::String(s.clone())),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(to_json(&*item.borrow(), span)?);
            }
            Ok(JsonValue::Array(out))
        }
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, vr) in map {
                out.insert(k.clone(), to_json(&*vr.borrow(), span)?);
            }
            Ok(JsonValue::Object(out))
        }
        other => Err(type_err(
            span,
            format!(
                "JSON values must be nil, bool, number, string, array, or object — got {}",
                other.type_name()
            ),
        )),
    }
}

fn from_json(v: JsonValue) -> ValueRef {
    match v {
        JsonValue::Null => Value::Nil.ref_cell(),
        JsonValue::Bool(b) => Value::Bool(b).ref_cell(),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i).ref_cell()
            } else if let Some(f) = n.as_f64() {
                Value::Float(f).ref_cell()
            } else {
                Value::String(n.to_string()).ref_cell()
            }
        }
        JsonValue::String(s) => Value::String(s).ref_cell(),
        JsonValue::Array(items) => {
            Value::Array(items.into_iter().map(from_json).collect()).ref_cell()
        }
        JsonValue::Object(map) => {
            let mut out = HashMap::new();
            for (k, v) in map {
                out.insert(k, from_json(v));
            }
            Value::Object(out).ref_cell()
        }
    }
}

fn json_arg(args: &[ValueRef], idx: usize, _name: &str, span: Span) -> NiaoResult<JsonValue> {
    to_json(&*args[idx].borrow(), span)
}

fn object_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Map<String, JsonValue>> {
    match json_arg(args, idx, name, span)? {
        JsonValue::Object(m) => Ok(m),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an object as argument {}, got {}",
                idx + 1,
                other
            ),
        )),
    }
}

fn optional_object(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<Option<Map<String, JsonValue>>> {
    if args.len() <= idx {
        return Ok(None);
    }
    match &*args[idx].borrow() {
        Value::Nil => Ok(None),
        Value::Object(_) => Ok(Some(object_arg(args, idx, "opts", span)?)),
        other => Err(type_err(
            span,
            format!("opts must be an object, got {}", other.type_name()),
        )),
    }
}

fn routes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<JsonValue>> {
    match &*args[idx].borrow() {
        Value::Array(items) => items
            .iter()
            .map(|v| to_json(&*v.borrow(), span))
            .collect(),
        Value::Nil => Ok(Vec::new()),
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

fn store_doc(doc: OpenApiDoc) -> ValueRef {
    let id = new_id();
    DOCS.with(|d| {
        d.borrow_mut().insert(id, doc);
    });
    Value::Int(id).ref_cell()
}

fn with_doc_mut<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&mut OpenApiDoc) -> NiaoResult<ValueRef>,
{
    DOCS.with(|d| {
        let mut map = d.borrow_mut();
        match map.get_mut(&id) {
            Some(doc) => f(doc),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn with_doc<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&OpenApiDoc) -> NiaoResult<ValueRef>,
{
    DOCS.with(|d| {
        let map = d.borrow();
        match map.get(&id) {
            Some(doc) => f(doc),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn clone_doc(id: i64, span: Span) -> NiaoResult<Result<OpenApiDoc, ValueRef>> {
    DOCS.with(|d| {
        let map = d.borrow();
        match map.get(&id) {
            Some(doc) => Ok(Ok(doc.clone())),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

// ---------------------------------------------------------------------------
// Document lifecycle
// ---------------------------------------------------------------------------

// >>> nopenapi.is_valid(nopenapi.create({title: "API", version: "1"}))
// => true
fn nopenapi_create(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nopenapi_create", span)?;
    let info = object_arg(args, 0, "nopenapi_create", span)?;
    let opts = optional_object(args, 1, span)?;
    match OpenApiDoc::create(&info, opts.as_ref()) {
        Ok(doc) => Ok(store_doc(doc)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> let _d = nopenapi.parse("{\"openapi\":\"3.1.0\",\"info\":{\"title\":\"T\",\"version\":\"1\"},\"paths\":{}}"); nopenapi.version(_d)
// => "3.1.0"
fn nopenapi_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_parse", span)?;
    match &*args[0].borrow() {
        Value::String(s) => match OpenApiDoc::parse_str(s) {
            Ok(doc) => Ok(store_doc(doc)),
            Err(e) => Ok(parse_err(span, e.message())),
        },
        Value::Object(_) => {
            let v = json_arg(args, 0, "nopenapi_parse", span)?;
            match OpenApiDoc::parse_value(v) {
                Ok(doc) => Ok(store_doc(doc)),
                Err(e) => Ok(parse_err(span, e.message())),
            }
        }
        other => Err(type_err(
            span,
            format!(
                "nopenapi_parse() expects string or object, got {}",
                other.type_name()
            ),
        )),
    }
}

fn nopenapi_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_load", span)?;
    let path = string_arg(args, 0, "nopenapi_load", span)?;
    match OpenApiDoc::load(&PathBuf::from(path)) {
        Ok(doc) => Ok(store_doc(doc)),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nopenapi_clone(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_clone", span)?;
    let id = handle_arg(args, 0, "nopenapi_clone", span)?;
    match clone_doc(id, span)? {
        Ok(doc) => Ok(store_doc(doc)),
        Err(e) => Ok(e),
    }
}

fn nopenapi_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_close", span)?;
    let id = handle_arg(args, 0, "nopenapi_close", span)?;
    let removed = DOCS.with(|d| d.borrow_mut().remove(&id).is_some());
    if removed {
        Ok(Value::Nil.ref_cell())
    } else {
        Ok(invalid_handle(span, id))
    }
}

fn nopenapi_save(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nopenapi_save", span)?;
    let id = handle_arg(args, 0, "nopenapi_save", span)?;
    let path = string_arg(args, 1, "nopenapi_save", span)?;
    let pretty = bool_arg(args, 2, true);
    with_doc(id, span, |doc| match doc.save(&PathBuf::from(&path), pretty) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    })
}

// >>> let d = nopenapi.create({title: "API", version: "1"}); nopenapi.add_route(d, {method: "GET", path: "/health"}); nopenapi.to_json(d).len() > 40
// => true
fn nopenapi_to_json(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nopenapi_to_json", span)?;
    let id = handle_arg(args, 0, "nopenapi_to_json", span)?;
    let pretty = bool_arg(args, 1, false);
    with_doc(id, span, |doc| match doc.to_json(pretty) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nopenapi_to_object(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_to_object", span)?;
    let id = handle_arg(args, 0, "nopenapi_to_object", span)?;
    with_doc(id, span, |doc| Ok(from_json(doc.to_value())))
}

fn nopenapi_version(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_version", span)?;
    let id = handle_arg(args, 0, "nopenapi_version", span)?;
    with_doc(id, span, |doc| {
        Ok(Value::String(doc.version().to_string()).ref_cell())
    })
}

// ---------------------------------------------------------------------------
// From routes / ahiru
// ---------------------------------------------------------------------------

fn nopenapi_from_routes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nopenapi_from_routes", span)?;
    let routes = routes_arg(args, 0, "nopenapi_from_routes", span)?;
    let info = optional_object(args, 1, span)?;
    let opts = optional_object(args, 2, span)?;
    match from_routes(&routes, info.as_ref(), opts.as_ref()) {
        Ok(doc) => Ok(store_doc(doc)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nopenapi.from_ahiru([{method: "GET", path: "/users/:id"}]).paths
// => ["/users/{id}"]
fn nopenapi_from_ahiru(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nopenapi_from_ahiru", span)?;
    let routes = routes_arg(args, 0, "nopenapi_from_ahiru", span)?;
    let info = optional_object(args, 1, span)?;
    let opts = optional_object(args, 2, span)?;
    match from_ahiru(&routes, info.as_ref(), opts.as_ref()) {
        Ok(doc) => Ok(store_doc(doc)),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nopenapi_add_route(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nopenapi_add_route", span)?;
    let id = handle_arg(args, 0, "nopenapi_add_route", span)?;
    let route = object_arg(args, 1, "nopenapi_add_route", span)?;
    with_doc_mut(id, span, |doc| match doc.add_route(&route) {
        Ok(()) => Ok(Value::Int(id).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nopenapi_add_routes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nopenapi_add_routes", span)?;
    let id = handle_arg(args, 0, "nopenapi_add_routes", span)?;
    let routes = routes_arg(args, 1, "nopenapi_add_routes", span)?;
    with_doc_mut(id, span, |doc| match doc.add_routes(&routes) {
        Ok(()) => Ok(Value::Int(id).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nopenapi_add_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "nopenapi_add_path", span)?;
    let id = handle_arg(args, 0, "nopenapi_add_path", span)?;
    let path = string_arg(args, 1, "nopenapi_add_path", span)?;
    let method = string_arg(args, 2, "nopenapi_add_path", span)?;
    let op = json_arg(args, 3, "nopenapi_add_path", span)?;
    with_doc_mut(id, span, |doc| match doc.add_path(&path, &method, op) {
        Ok(()) => Ok(Value::Int(id).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nopenapi_add_server(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nopenapi_add_server", span)?;
    let id = handle_arg(args, 0, "nopenapi_add_server", span)?;
    let url = string_arg(args, 1, "nopenapi_add_server", span)?;
    let desc = if args.len() > 2 {
        match &*args[2].borrow() {
            Value::String(s) => Some(s.clone()),
            Value::Nil => None,
            other => {
                return Err(type_err(
                    span,
                    format!("description must be string, got {}", other.type_name()),
                ))
            }
        }
    } else {
        None
    };
    with_doc_mut(id, span, |doc| {
        doc.add_server(&url, desc.as_deref());
        Ok(Value::Int(id).ref_cell())
    })
}

fn nopenapi_add_tag(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nopenapi_add_tag", span)?;
    let id = handle_arg(args, 0, "nopenapi_add_tag", span)?;
    let name = string_arg(args, 1, "nopenapi_add_tag", span)?;
    let desc = if args.len() > 2 {
        match &*args[2].borrow() {
            Value::String(s) => Some(s.clone()),
            Value::Nil => None,
            other => {
                return Err(type_err(
                    span,
                    format!("description must be string, got {}", other.type_name()),
                ))
            }
        }
    } else {
        None
    };
    with_doc_mut(id, span, |doc| {
        doc.add_tag(&name, desc.as_deref());
        Ok(Value::Int(id).ref_cell())
    })
}

fn nopenapi_add_schema(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nopenapi_add_schema", span)?;
    let id = handle_arg(args, 0, "nopenapi_add_schema", span)?;
    let name = string_arg(args, 1, "nopenapi_add_schema", span)?;
    let schema = json_arg(args, 2, "nopenapi_add_schema", span)?;
    with_doc_mut(id, span, |doc| match doc.add_schema(&name, schema) {
        Ok(()) => Ok(Value::Int(id).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nopenapi_add_security_scheme(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nopenapi_add_security_scheme", span)?;
    let id = handle_arg(args, 0, "nopenapi_add_security_scheme", span)?;
    let name = string_arg(args, 1, "nopenapi_add_security_scheme", span)?;
    let scheme = json_arg(args, 2, "nopenapi_add_security_scheme", span)?;
    with_doc_mut(id, span, |doc| match doc.add_security_scheme(&name, scheme) {
        Ok(()) => Ok(Value::Int(id).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nopenapi_add_component(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "nopenapi_add_component", span)?;
    let id = handle_arg(args, 0, "nopenapi_add_component", span)?;
    let kind = string_arg(args, 1, "nopenapi_add_component", span)?;
    let name = string_arg(args, 2, "nopenapi_add_component", span)?;
    let value = json_arg(args, 3, "nopenapi_add_component", span)?;
    with_doc_mut(id, span, |doc| match doc.add_component(&kind, &name, value) {
        Ok(()) => Ok(Value::Int(id).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nopenapi_set_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nopenapi_set_info", span)?;
    let id = handle_arg(args, 0, "nopenapi_set_info", span)?;
    let info = object_arg(args, 1, "nopenapi_set_info", span)?;
    with_doc_mut(id, span, |doc| {
        doc.set_info(&info);
        Ok(Value::Int(id).ref_cell())
    })
}

fn nopenapi_merge(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nopenapi_merge", span)?;
    let a = handle_arg(args, 0, "nopenapi_merge", span)?;
    let b = handle_arg(args, 1, "nopenapi_merge", span)?;
    let left = match clone_doc(a, span)? {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    let right = match clone_doc(b, span)? {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    match merge(&left, &right) {
        Ok(doc) => Ok(store_doc(doc)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn nopenapi_operation(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_operation", span)?;
    let opts = object_arg(args, 0, "nopenapi_operation", span)?;
    Ok(from_json(operation(&opts)))
}

fn nopenapi_param(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nopenapi_param", span)?;
    let name = string_arg(args, 0, "nopenapi_param", span)?;
    let location = string_arg(args, 1, "nopenapi_param", span)?;
    let schema = if args.len() > 2 && !matches!(&*args[2].borrow(), Value::Nil) {
        Some(json_arg(args, 2, "nopenapi_param", span)?)
    } else {
        None
    };
    let opts = optional_object(args, 3, span)?;
    Ok(from_json(param(
        &name,
        &location,
        schema,
        opts.as_ref(),
    )))
}

fn nopenapi_request_body(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nopenapi_request_body", span)?;
    let schema = json_arg(args, 0, "nopenapi_request_body", span)?;
    let opts = optional_object(args, 1, span)?;
    Ok(from_json(request_body(schema, opts.as_ref())))
}

fn nopenapi_response(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nopenapi_response", span)?;
    let status = match &*args[0].borrow() {
        Value::String(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        other => {
            return Err(type_err(
                span,
                format!("status must be string or int, got {}", other.type_name()),
            ))
        }
    };
    let description = string_arg(args, 1, "nopenapi_response", span)?;
    let schema = if args.len() > 2 && !matches!(&*args[2].borrow(), Value::Nil) {
        Some(json_arg(args, 2, "nopenapi_response", span)?)
    } else {
        None
    };
    let opts = optional_object(args, 3, span)?;
    let (st, resp) = response(&status, &description, schema, opts.as_ref());
    let mut m = HashMap::new();
    m.insert("status".into(), Value::String(st).ref_cell());
    m.insert("response".into(), from_json(resp));
    Ok(Value::Object(m).ref_cell())
}

// >>> nopenapi.schema_ref("User").$ref
// => "#/components/schemas/User"
fn nopenapi_schema_ref(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_schema_ref", span)?;
    let name = string_arg(args, 0, "nopenapi_schema_ref", span)?;
    Ok(from_json(schema_ref(&name)))
}

fn nopenapi_schema_object(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nopenapi_schema_object", span)?;
    let props = object_arg(args, 0, "nopenapi_schema_object", span)?;
    let required = if args.len() > 1 {
        match &*args[1].borrow() {
            Value::Nil => None,
            Value::Array(items) => {
                let mut names = Vec::new();
                for it in items {
                    match &*it.borrow() {
                        Value::String(s) => names.push(s.clone()),
                        other => {
                            return Err(type_err(
                                span,
                                format!("required[] items must be strings, got {}", other.type_name()),
                            ))
                        }
                    }
                }
                Some(names)
            }
            other => {
                return Err(type_err(
                    span,
                    format!("required must be an array, got {}", other.type_name()),
                ))
            }
        }
    } else {
        None
    };
    let opts = optional_object(args, 2, span)?;
    Ok(from_json(schema_object(props, required, opts.as_ref())))
}

fn nopenapi_schema_array(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nopenapi_schema_array", span)?;
    let items = json_arg(args, 0, "nopenapi_schema_array", span)?;
    let opts = optional_object(args, 1, span)?;
    Ok(from_json(schema_array(items, opts.as_ref())))
}

fn nopenapi_schema_string(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nopenapi_schema_string", span)?;
    let opts = optional_object(args, 0, span)?;
    Ok(from_json(schema_string(opts.as_ref())))
}

fn nopenapi_schema_integer(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nopenapi_schema_integer", span)?;
    let opts = optional_object(args, 0, span)?;
    Ok(from_json(schema_integer(opts.as_ref())))
}

fn nopenapi_schema_number(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nopenapi_schema_number", span)?;
    let opts = optional_object(args, 0, span)?;
    Ok(from_json(schema_number(opts.as_ref())))
}

fn nopenapi_schema_boolean(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nopenapi_schema_boolean", span)?;
    let opts = optional_object(args, 0, span)?;
    Ok(from_json(schema_boolean(opts.as_ref())))
}

// >>> nopenapi.infer_schema({id: 1, name: "a"}).type
// => "object"
fn nopenapi_infer_schema(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_infer_schema", span)?;
    let v = json_arg(args, 0, "nopenapi_infer_schema", span)?;
    Ok(from_json(infer_schema(&v)))
}

// ---------------------------------------------------------------------------
// Introspection / validate / client
// ---------------------------------------------------------------------------

fn nopenapi_paths(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_paths", span)?;
    let id = handle_arg(args, 0, "nopenapi_paths", span)?;
    with_doc(id, span, |doc| {
        let items: Vec<ValueRef> = doc
            .paths()
            .into_iter()
            .map(|p| Value::String(p).ref_cell())
            .collect();
        Ok(Value::Array(items).ref_cell())
    })
}

fn nopenapi_operations(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_operations", span)?;
    let id = handle_arg(args, 0, "nopenapi_operations", span)?;
    with_doc(id, span, |doc| {
        Ok(Value::Array(doc.operations().into_iter().map(from_json).collect()).ref_cell())
    })
}

fn nopenapi_get_operation(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nopenapi_get_operation", span)?;
    let id = handle_arg(args, 0, "nopenapi_get_operation", span)?;
    let path = string_arg(args, 1, "nopenapi_get_operation", span)?;
    let method = string_arg(args, 2, "nopenapi_get_operation", span)?;
    with_doc(id, span, |doc| match doc.get_operation(&path, &method) {
        Ok(Some(op)) => Ok(from_json(op)),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nopenapi_schemas(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_schemas", span)?;
    let id = handle_arg(args, 0, "nopenapi_schemas", span)?;
    with_doc(id, span, |doc| {
        let items: Vec<ValueRef> = doc
            .schemas()
            .into_iter()
            .map(|p| Value::String(p).ref_cell())
            .collect();
        Ok(Value::Array(items).ref_cell())
    })
}

// >>> nopenapi.path_params("/users/:id/posts/:pid")
// => ["id", "pid"]
fn nopenapi_path_params(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_path_params", span)?;
    let path = string_arg(args, 0, "nopenapi_path_params", span)?;
    let items: Vec<ValueRef> = path_params(&path)
        .into_iter()
        .map(|p| Value::String(p).ref_cell())
        .collect();
    Ok(Value::Array(items).ref_cell())
}

// >>> nopenapi.normalize_path("/users/:id")
// => "/users/{id}"
fn nopenapi_normalize_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_normalize_path", span)?;
    let path = string_arg(args, 0, "nopenapi_normalize_path", span)?;
    Ok(Value::String(normalize_path(&path)).ref_cell())
}

fn nopenapi_operation_id(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nopenapi_operation_id", span)?;
    let method = string_arg(args, 0, "nopenapi_operation_id", span)?;
    let path = string_arg(args, 1, "nopenapi_operation_id", span)?;
    match operation_id(&method, &path) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nopenapi.validate(nopenapi.create({title: "T", version: "1"})).ok
// => true
fn nopenapi_validate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_validate", span)?;
    let id = handle_arg(args, 0, "nopenapi_validate", span)?;
    with_doc(id, span, |doc| match validate(doc) {
        Ok(r) => Ok(from_json(r.to_value())),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nopenapi_is_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nopenapi_is_valid", span)?;
    let id = handle_arg(args, 0, "nopenapi_is_valid", span)?;
    with_doc(id, span, |doc| match is_valid(doc) {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    })
}

// >>> let d = nopenapi.from_routes([{method: "GET", path: "/x"}]); nopenapi.client_stub(d).contains("http.get")
// => true
fn nopenapi_client_stub(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nopenapi_client_stub", span)?;
    let id = handle_arg(args, 0, "nopenapi_client_stub", span)?;
    let opts = optional_object(args, 1, span)?;
    with_doc(id, span, |doc| match client_stub_str(doc, opts.as_ref()) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nopenapi_client_niao(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nopenapi_client_stub(args, span)
}

fn nopenapi_parallel_validate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nopenapi_parallel_validate", span)?;
    let handles = match &*args[0].borrow() {
        Value::Array(items) => {
            let mut ids = Vec::new();
            for (i, it) in items.iter().enumerate() {
                match &*it.borrow() {
                    Value::Int(id) if *id > 0 => ids.push(*id),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "nopenapi_parallel_validate() docs[{i}] must be a handle, got {}",
                                other.type_name()
                            ),
                        ))
                    }
                }
            }
            ids
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "nopenapi_parallel_validate() expects an array, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    let threads = if args.len() > 1 {
        match &*args[1].borrow() {
            Value::Object(m) => m
                .get("threads")
                .and_then(|v| match &*v.borrow() {
                    Value::Int(n) => Some(*n as usize),
                    _ => None,
                })
                .unwrap_or_else(available_threads),
            Value::Int(n) => *n as usize,
            Value::Nil => available_threads(),
            _ => available_threads(),
        }
    } else {
        available_threads()
    };
    let mut docs = Vec::with_capacity(handles.len());
    for id in &handles {
        match clone_doc(*id, span)? {
            Ok(d) => docs.push(d),
            Err(e) => return Ok(e),
        }
    }
    let reports = parallel_validate(&docs, threads);
    Ok(Value::Array(
        reports
            .into_iter()
            .map(|r| from_json(r.to_value()))
            .collect(),
    )
    .ref_cell())
}

fn nopenapi_parallel_client_stubs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nopenapi_parallel_client_stubs", span)?;
    let handles = match &*args[0].borrow() {
        Value::Array(items) => {
            let mut ids = Vec::new();
            for (i, it) in items.iter().enumerate() {
                match &*it.borrow() {
                    Value::Int(id) if *id > 0 => ids.push(*id),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "nopenapi_parallel_client_stubs() docs[{i}] must be a handle, got {}",
                                other.type_name()
                            ),
                        ))
                    }
                }
            }
            ids
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "nopenapi_parallel_client_stubs() expects an array, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    let opts = optional_object(args, 1, span)?;
    let threads = opts
        .as_ref()
        .and_then(|m| m.get("threads"))
        .and_then(|v| v.as_i64())
        .map(|n| n as usize)
        .unwrap_or_else(available_threads);
    let mut docs = Vec::with_capacity(handles.len());
    for id in &handles {
        match clone_doc(*id, span)? {
            Ok(d) => docs.push(d),
            Err(e) => return Ok(e),
        }
    }
    let stubs = parallel_client_stubs(&docs, opts.as_ref(), threads);
    let items: Vec<ValueRef> = stubs
        .into_iter()
        .map(|r| match r {
            Ok(s) => Value::String(s).ref_cell(),
            Err(e) => nopenapi_err(span, e),
        })
        .collect();
    Ok(Value::Array(items).ref_cell())
}

macro_rules! nopenapi_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nopenapi_fns![
    ("nopenapi_create", "create", nopenapi_create),
    ("nopenapi_parse", "parse", nopenapi_parse),
    ("nopenapi_load", "load", nopenapi_load),
    ("nopenapi_clone", "clone", nopenapi_clone),
    ("nopenapi_close", "close", nopenapi_close),
    ("nopenapi_save", "save", nopenapi_save),
    ("nopenapi_to_json", "to_json", nopenapi_to_json),
    ("nopenapi_to_object", "to_object", nopenapi_to_object),
    ("nopenapi_version", "version", nopenapi_version),
    ("nopenapi_from_routes", "from_routes", nopenapi_from_routes),
    ("nopenapi_from_ahiru", "from_ahiru", nopenapi_from_ahiru),
    ("nopenapi_add_route", "add_route", nopenapi_add_route),
    ("nopenapi_add_routes", "add_routes", nopenapi_add_routes),
    ("nopenapi_add_path", "add_path", nopenapi_add_path),
    ("nopenapi_add_server", "add_server", nopenapi_add_server),
    ("nopenapi_add_tag", "add_tag", nopenapi_add_tag),
    ("nopenapi_add_schema", "add_schema", nopenapi_add_schema),
    ("nopenapi_add_security_scheme", "add_security_scheme", nopenapi_add_security_scheme),
    ("nopenapi_add_component", "add_component", nopenapi_add_component),
    ("nopenapi_set_info", "set_info", nopenapi_set_info),
    ("nopenapi_merge", "merge", nopenapi_merge),
    ("nopenapi_operation", "operation", nopenapi_operation),
    ("nopenapi_param", "param", nopenapi_param),
    ("nopenapi_request_body", "request_body", nopenapi_request_body),
    ("nopenapi_response", "response", nopenapi_response),
    ("nopenapi_schema_ref", "schema_ref", nopenapi_schema_ref),
    ("nopenapi_schema_object", "schema_object", nopenapi_schema_object),
    ("nopenapi_schema_array", "schema_array", nopenapi_schema_array),
    ("nopenapi_schema_string", "schema_string", nopenapi_schema_string),
    ("nopenapi_schema_integer", "schema_integer", nopenapi_schema_integer),
    ("nopenapi_schema_number", "schema_number", nopenapi_schema_number),
    ("nopenapi_schema_boolean", "schema_boolean", nopenapi_schema_boolean),
    ("nopenapi_infer_schema", "infer_schema", nopenapi_infer_schema),
    ("nopenapi_paths", "paths", nopenapi_paths),
    ("nopenapi_operations", "operations", nopenapi_operations),
    ("nopenapi_get_operation", "get_operation", nopenapi_get_operation),
    ("nopenapi_schemas", "schemas", nopenapi_schemas),
    ("nopenapi_path_params", "path_params", nopenapi_path_params),
    ("nopenapi_normalize_path", "normalize_path", nopenapi_normalize_path),
    ("nopenapi_operation_id", "operation_id", nopenapi_operation_id),
    ("nopenapi_validate", "validate", nopenapi_validate),
    ("nopenapi_is_valid", "is_valid", nopenapi_is_valid),
    ("nopenapi_client_stub", "client_stub", nopenapi_client_stub),
    ("nopenapi_client_niao", "client_niao", nopenapi_client_niao),
    ("nopenapi_parallel_validate", "parallel_validate", nopenapi_parallel_validate),
    ("nopenapi_parallel_client_stubs", "parallel_client_stubs", nopenapi_parallel_client_stubs),
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

pub const MODULE_NAME: &str = "nopenapi";
pub const MODULE_PATHS: &[&str] = &["nopenapi", "std/nopenapi"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    fn obj(pairs: &[(&str, Value)]) -> ValueRef {
        let mut m = HashMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), v.clone().ref_cell());
        }
        Value::Object(m).ref_cell()
    }

    #[test]
    fn create_from_ahiru_doctest() {
        let routes = Value::Array(vec![obj(&[
            ("method", Value::String("GET".into())),
            ("path", Value::String("/users/:id".into())),
        ])])
        .ref_cell();
        let doc = nopenapi_from_ahiru(&[routes], span()).unwrap();
        let paths = nopenapi_paths(&[doc.clone()], span()).unwrap();
        match &*paths.borrow() {
            Value::Array(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(*items[0].borrow(), Value::String("/users/{id}".into()));
            }
            other => panic!("expected array, got {other:?}"),
        }
        let _ = nopenapi_close(&[doc], span());
    }

    #[test]
    fn normalize_and_infer() {
        let p = nopenapi_normalize_path(&[Value::String("/a/:b".into()).ref_cell()], span())
            .unwrap();
        assert_eq!(*p.borrow(), Value::String("/a/{b}".into()));
        let schema = nopenapi_infer_schema(
            &[obj(&[("id", Value::Int(1)), ("name", Value::String("x".into()))])],
            span(),
        )
        .unwrap();
        match &*schema.borrow() {
            Value::Object(m) => {
                assert_eq!(*m.get("type").unwrap().borrow(), Value::String("object".into()));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn client_stub_smoke() {
        let info = obj(&[
            ("title", Value::String("API".into())),
            ("version", Value::String("1".into())),
        ]);
        let doc = nopenapi_create(&[info], span()).unwrap();
        let route = obj(&[
            ("method", Value::String("GET".into())),
            ("path", Value::String("/health".into())),
        ]);
        let _ = nopenapi_add_route(&[doc.clone(), route], span()).unwrap();
        let stub = nopenapi_client_stub(&[doc.clone()], span()).unwrap();
        match &*stub.borrow() {
            Value::String(s) => {
                assert!(s.contains("import \"http\""));
                assert!(s.contains("http.get"));
            }
            other => panic!("expected string, got {other:?}"),
        }
        let _ = nopenapi_close(&[doc], span());
    }

    #[test]
    fn invalid_json_parse() {
        let v = nopenapi_parse(&[Value::String("{".into()).ref_cell()], span()).unwrap();
        let borrowed = v.borrow();
        match &*borrowed {
            Value::Error(e) => assert_eq!(e.code, E4123),
            other => panic!("expected error, got {other:?}"),
        }
    }

    #[test]
    fn closed_handle() {
        let info = obj(&[
            ("title", Value::String("T".into())),
            ("version", Value::String("1".into())),
        ]);
        let doc = nopenapi_create(&[info], span()).unwrap();
        let _ = nopenapi_close(&[doc.clone()], span()).unwrap();
        let v = nopenapi_version(&[doc], span()).unwrap();
        let borrowed = v.borrow();
        match &*borrowed {
            Value::Error(e) => assert_eq!(e.code, E4124),
            other => panic!("expected error, got {other:?}"),
        }
    }
}
