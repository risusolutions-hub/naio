//! Native njpath standard library — JSONPath / JMESPath queries, JSON Pointer
//! and JSON Patch over values (~jmespath, jsonpath-ng, glom subset).
//!
//! Import with `import "njpath"` (or `import "std/njpath"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_bignum::BigInt;
use niao_errors::codes;
use niao_jpath::{
    compile_jmes, compile_path, diff, jmes, merge_patch, parallel_find, parallel_find_one,
    parallel_jmes, patch_apply, patch_op_names, patch_test, patch_valid, path_delete, path_find,
    path_find_one, path_replace, path_search, path_valid, pointer_escape, pointer_exists,
    pointer_get, pointer_join, pointer_parent, pointer_remove, pointer_resolve, pointer_set,
    pointer_test, pointer_unescape, CompiledJmes, CompiledJsonPath, JpathError, ParallelOpts,
};
use niao_parallel::available_threads;
use serde_json::{Map, Value as JsonValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

enum HandleKind {
    JsonPath(CompiledJsonPath),
    Jmes(CompiledJmes),
}

thread_local! {
    static HANDLES: RefCell<HashMap<i64, HandleKind>> = RefCell::new(HashMap::new());
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

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4382_NJPATH_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E4380_NJPATH_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4380_NJPATH_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn njpath_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4381_NJPATH_ERROR, "njpath_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        codes::E4383_NJPATH_INVALID_HANDLE,
        "njpath_error",
        format!("invalid or closed njpath handle {id}"),
        span,
    )
}

fn map_jpath_err(span: Span, e: JpathError) -> ValueRef {
    njpath_err(span, e.message())
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

fn json_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<JsonValue> {
    to_json(&*args[idx].borrow(), span)
}

fn parse_opts(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return Ok(HashMap::new());
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        Value::Nil => Ok(HashMap::new()),
        other => Err(type_err(
            span,
            format!(
                "expected options object, got {}",
                other.type_name()
            ),
        )),
    }
}

fn obj_int(map: &HashMap<String, ValueRef>, key: &str, default: i64) -> i64 {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => n,
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
        Value::BigInt(n) => bigint_to_json(n, span),
        other => Err(type_err(
            span,
            format!(
                "JSON values must be nil, bool, number, string, array, or object — got {}",
                other.type_name()
            ),
        )),
    }
}

fn bigint_to_json(n: &BigInt, span: Span) -> NiaoResult<JsonValue> {
    let s = n.to_string();
    if let Ok(i) = s.parse::<i64>() {
        return Ok(JsonValue::Number(i.into()));
    }
    if let Ok(u) = s.parse::<u64>() {
        return Ok(JsonValue::Number(u.into()));
    }
    if let Ok(f) = s.parse::<f64>() {
        if f.is_finite() {
            if let Some(num) = serde_json::Number::from_f64(f) {
                return Ok(JsonValue::Number(num));
            }
        }
    }
    Err(type_err(span, format!("bigint {s} is not JSON-representable")))
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
            let out: Vec<ValueRef> = items.into_iter().map(from_json).collect();
            Value::Array(out).ref_cell()
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

fn json_array(items: Vec<JsonValue>, span: Span) -> NiaoResult<ValueRef> {
    Ok(Value::Array(items.into_iter().map(from_json).collect()).ref_cell())
}

fn json_docs_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<JsonValue>> {
    match &*args[idx].borrow() {
        Value::Array(items) => items
            .iter()
            .map(|v| to_json(&*v.borrow(), span))
            .collect(),
        Value::Nil => Ok(Vec::new()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an array of documents as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn parallel_opts(map: &HashMap<String, ValueRef>) -> ParallelOpts {
    ParallelOpts {
        threads: obj_int(map, "threads", available_threads() as i64).max(1) as usize,
    }
}

// ---------------------------------------------------------------------------
// JSON Pointer
// ---------------------------------------------------------------------------

fn njpath_pointer_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_pointer_get", span)?;
    let doc = json_arg(args, 0, "njpath_pointer_get", span)?;
    let ptr = string_arg(args, 1, "njpath_pointer_get", span)?;
    match pointer_get(&doc, &ptr) {
        Ok(v) => Ok(from_json(v)),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_pointer_resolve(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_pointer_resolve", span)?;
    let doc = json_arg(args, 0, "njpath_pointer_resolve", span)?;
    let ptr = string_arg(args, 1, "njpath_pointer_resolve", span)?;
    match pointer_resolve(&doc, &ptr) {
        Ok(v) => Ok(from_json(v)),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_pointer_exists(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_pointer_exists", span)?;
    let doc = json_arg(args, 0, "njpath_pointer_exists", span)?;
    let ptr = string_arg(args, 1, "njpath_pointer_exists", span)?;
    match pointer_exists(&doc, &ptr) {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_pointer_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "njpath_pointer_set", span)?;
    let doc = json_arg(args, 0, "njpath_pointer_set", span)?;
    let ptr = string_arg(args, 1, "njpath_pointer_set", span)?;
    let value = json_arg(args, 2, "njpath_pointer_set", span)?;
    match pointer_set(&doc, &ptr, value) {
        Ok(v) => Ok(from_json(v)),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_pointer_remove(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_pointer_remove", span)?;
    let doc = json_arg(args, 0, "njpath_pointer_remove", span)?;
    let ptr = string_arg(args, 1, "njpath_pointer_remove", span)?;
    match pointer_remove(&doc, &ptr) {
        Ok(v) => Ok(from_json(v)),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_pointer_test(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "njpath_pointer_test", span)?;
    let doc = json_arg(args, 0, "njpath_pointer_test", span)?;
    let ptr = string_arg(args, 1, "njpath_pointer_test", span)?;
    let expected = json_arg(args, 2, "njpath_pointer_test", span)?;
    match pointer_test(&doc, &ptr, &expected) {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_pointer_join(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_pointer_join", span)?;
    let base = string_arg(args, 0, "njpath_pointer_join", span)?;
    let token = string_arg(args, 1, "njpath_pointer_join", span)?;
    match pointer_join(&base, &token) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_pointer_parent(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "njpath_pointer_parent", span)?;
    let ptr = string_arg(args, 0, "njpath_pointer_parent", span)?;
    match pointer_parent(&ptr) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_pointer_escape(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "njpath_pointer_escape", span)?;
    Ok(Value::String(pointer_escape(&string_arg(args, 0, "njpath_pointer_escape", span)?)).ref_cell())
}

fn njpath_pointer_unescape(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "njpath_pointer_unescape", span)?;
    Ok(Value::String(pointer_unescape(&string_arg(args, 0, "njpath_pointer_unescape", span)?)).ref_cell())
}

// ---------------------------------------------------------------------------
// JSON Patch
// ---------------------------------------------------------------------------

fn njpath_patch_apply(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_patch_apply", span)?;
    let doc = json_arg(args, 0, "njpath_patch_apply", span)?;
    let patch = json_arg(args, 1, "njpath_patch_apply", span)?;
    match patch_apply(&doc, &patch) {
        Ok(v) => Ok(from_json(v)),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_patch_test(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_patch_test", span)?;
    let doc = json_arg(args, 0, "njpath_patch_test", span)?;
    let patch = json_arg(args, 1, "njpath_patch_test", span)?;
    match patch_test(&doc, &patch) {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_patch_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "njpath_patch_valid", span)?;
    let patch = json_arg(args, 0, "njpath_patch_valid", span)?;
    Ok(Value::Bool(patch_valid(&patch)).ref_cell())
}

fn njpath_diff(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_diff", span)?;
    let a = json_arg(args, 0, "njpath_diff", span)?;
    let b = json_arg(args, 1, "njpath_diff", span)?;
    Ok(from_json(diff(&a, &b)))
}

fn njpath_merge(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_merge", span)?;
    let doc = json_arg(args, 0, "njpath_merge", span)?;
    let patch = json_arg(args, 1, "njpath_merge", span)?;
    match merge_patch(&doc, &patch) {
        Ok(v) => Ok(from_json(v)),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_patch_op_names(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "njpath_patch_op_names", span)?;
    let patch = json_arg(args, 0, "njpath_patch_op_names", span)?;
    match patch_op_names(&patch) {
        Ok(names) => Ok(Value::Array(names.into_iter().map(|s| Value::String(s).ref_cell()).collect()).ref_cell()),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// JSONPath
// ---------------------------------------------------------------------------

fn njpath_find(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_find", span)?;
    let doc = json_arg(args, 0, "njpath_find", span)?;
    let query = string_arg(args, 1, "njpath_find", span)?;
    match path_find(&doc, &query) {
        Ok(hits) => json_array(hits, span),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_find_one(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_find_one", span)?;
    let doc = json_arg(args, 0, "njpath_find_one", span)?;
    let query = string_arg(args, 1, "njpath_find_one", span)?;
    match path_find_one(&doc, &query) {
        Ok(v) => Ok(from_json(v)),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_find_paths(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_find_paths", span)?;
    let doc = json_arg(args, 0, "njpath_find_paths", span)?;
    let query = string_arg(args, 1, "njpath_find_paths", span)?;
    match niao_jpath::find_pointers(&doc, &query) {
        Ok(paths) => Ok(Value::Array(paths.into_iter().map(|s| Value::String(s).ref_cell()).collect()).ref_cell()),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_path_replace(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "njpath_path_replace", span)?;
    let doc = json_arg(args, 0, "njpath_path_replace", span)?;
    let query = string_arg(args, 1, "njpath_path_replace", span)?;
    let value = json_arg(args, 2, "njpath_path_replace", span)?;
    match path_replace(&doc, &query, &value) {
        Ok(v) => Ok(from_json(v)),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_path_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_path_delete", span)?;
    let doc = json_arg(args, 0, "njpath_path_delete", span)?;
    let query = string_arg(args, 1, "njpath_path_delete", span)?;
    match path_delete(&doc, &query) {
        Ok(v) => Ok(from_json(v)),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_path_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "njpath_path_valid", span)?;
    Ok(Value::Bool(path_valid(&string_arg(args, 0, "njpath_path_valid", span)?)).ref_cell())
}

fn njpath_compile_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "njpath_compile_path", span)?;
    let query = string_arg(args, 0, "njpath_compile_path", span)?;
    match compile_path(&query) {
        Ok(c) => {
            let id = new_handle();
            HANDLES.with(|m| m.borrow_mut().insert(id, HandleKind::JsonPath(c)));
            Ok(Value::Int(id).ref_cell())
        }
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_path_search(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_path_search", span)?;
    let id = handle_arg(args, 0, "njpath_path_search", span)?;
    let doc = json_arg(args, 1, "njpath_path_search", span)?;
    HANDLES.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(HandleKind::JsonPath(c)) => match path_search(c, &doc) {
                Ok(hits) => json_array(hits, span),
                Err(e) => Ok(map_jpath_err(span, e)),
            },
            Some(_) => Ok(invalid_handle(span, id)),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn njpath_path_query(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "njpath_path_query", span)?;
    let id = handle_arg(args, 0, "njpath_path_query", span)?;
    HANDLES.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(HandleKind::JsonPath(c)) => Ok(Value::String(c.query().to_string()).ref_cell()),
            Some(_) => Ok(invalid_handle(span, id)),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// ---------------------------------------------------------------------------
// JMESPath
// ---------------------------------------------------------------------------

fn njpath_jmes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_jmes", span)?;
    let doc = json_arg(args, 0, "njpath_jmes", span)?;
    let expr = string_arg(args, 1, "njpath_jmes", span)?;
    match jmes(&doc, &expr) {
        Ok(v) => Ok(from_json(v)),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_jmes_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "njpath_jmes_valid", span)?;
    Ok(Value::Bool(niao_jpath::jmes_valid(&string_arg(args, 0, "njpath_jmes_valid", span)?)).ref_cell())
}

fn njpath_compile_jmes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "njpath_compile_jmes", span)?;
    let expr = string_arg(args, 0, "njpath_compile_jmes", span)?;
    match compile_jmes(&expr) {
        Ok(c) => {
            let id = new_handle();
            HANDLES.with(|m| m.borrow_mut().insert(id, HandleKind::Jmes(c)));
            Ok(Value::Int(id).ref_cell())
        }
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_jmes_search(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "njpath_jmes_search", span)?;
    let id = handle_arg(args, 0, "njpath_jmes_search", span)?;
    let doc = json_arg(args, 1, "njpath_jmes_search", span)?;
    HANDLES.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(HandleKind::Jmes(c)) => match niao_jpath::search_with_compiled(c, &doc) {
                Ok(v) => Ok(from_json(v)),
                Err(e) => Ok(map_jpath_err(span, e)),
            },
            Some(_) => Ok(invalid_handle(span, id)),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn njpath_jmes_expression(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "njpath_jmes_expression", span)?;
    let id = handle_arg(args, 0, "njpath_jmes_expression", span)?;
    HANDLES.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(HandleKind::Jmes(c)) => Ok(Value::String(c.expression().to_string()).ref_cell()),
            Some(_) => Ok(invalid_handle(span, id)),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn njpath_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "njpath_close", span)?;
    let id = handle_arg(args, 0, "njpath_close", span)?;
    HANDLES.with(|m| {
        if m.borrow_mut().remove(&id).is_some() {
            Ok(Value::Nil.ref_cell())
        } else {
            Ok(invalid_handle(span, id))
        }
    })
}

// ---------------------------------------------------------------------------
// Parallel
// ---------------------------------------------------------------------------

fn njpath_parallel_find(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "njpath_parallel_find", span)?;
    let docs = json_docs_arg(args, 0, "njpath_parallel_find", span)?;
    let query = string_arg(args, 1, "njpath_parallel_find", span)?;
    let opts = parallel_opts(&parse_opts(args, 2, span)?);
    match parallel_find(&docs, &query, &opts) {
        Ok(rows) => {
            let outer: NiaoResult<Vec<ValueRef>> = rows
                .into_iter()
                .map(|row| json_array(row, span))
                .collect();
            Ok(Value::Array(outer?).ref_cell())
        }
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_parallel_find_one(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "njpath_parallel_find_one", span)?;
    let docs = json_docs_arg(args, 0, "njpath_parallel_find_one", span)?;
    let query = string_arg(args, 1, "njpath_parallel_find_one", span)?;
    let opts = parallel_opts(&parse_opts(args, 2, span)?);
    match parallel_find_one(&docs, &query, &opts) {
        Ok(rows) => json_array(rows, span),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

fn njpath_parallel_jmes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "njpath_parallel_jmes", span)?;
    let docs = json_docs_arg(args, 0, "njpath_parallel_jmes", span)?;
    let expr = string_arg(args, 1, "njpath_parallel_jmes", span)?;
    let opts = parallel_opts(&parse_opts(args, 2, span)?);
    match parallel_jmes(&docs, &expr, &opts) {
        Ok(rows) => json_array(rows, span),
        Err(e) => Ok(map_jpath_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! njpath_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

njpath_fns![
    ("njpath_pointer_get", "pointer_get", njpath_pointer_get),
    ("njpath_pointer_resolve", "pointer_resolve", njpath_pointer_resolve),
    ("njpath_pointer_exists", "pointer_exists", njpath_pointer_exists),
    ("njpath_pointer_set", "pointer_set", njpath_pointer_set),
    ("njpath_pointer_remove", "pointer_remove", njpath_pointer_remove),
    ("njpath_pointer_test", "pointer_test", njpath_pointer_test),
    ("njpath_pointer_join", "pointer_join", njpath_pointer_join),
    ("njpath_pointer_parent", "pointer_parent", njpath_pointer_parent),
    ("njpath_pointer_escape", "pointer_escape", njpath_pointer_escape),
    ("njpath_pointer_unescape", "pointer_unescape", njpath_pointer_unescape),
    ("njpath_patch_apply", "patch_apply", njpath_patch_apply),
    ("njpath_patch_test", "patch_test", njpath_patch_test),
    ("njpath_patch_valid", "patch_valid", njpath_patch_valid),
    ("njpath_diff", "diff", njpath_diff),
    ("njpath_merge", "merge", njpath_merge),
    ("njpath_patch_op_names", "patch_op_names", njpath_patch_op_names),
    ("njpath_find", "find", njpath_find),
    ("njpath_find_one", "find_one", njpath_find_one),
    ("njpath_find_paths", "find_paths", njpath_find_paths),
    ("njpath_path_replace", "path_replace", njpath_path_replace),
    ("njpath_path_delete", "path_delete", njpath_path_delete),
    ("njpath_path_valid", "path_valid", njpath_path_valid),
    ("njpath_compile_path", "compile_path", njpath_compile_path),
    ("njpath_path_search", "path_search", njpath_path_search),
    ("njpath_path_query", "path_query", njpath_path_query),
    ("njpath_jmes", "jmes", njpath_jmes),
    ("njpath_jmes_valid", "jmes_valid", njpath_jmes_valid),
    ("njpath_compile_jmes", "compile_jmes", njpath_compile_jmes),
    ("njpath_jmes_search", "jmes_search", njpath_jmes_search),
    ("njpath_jmes_expression", "jmes_expression", njpath_jmes_expression),
    ("njpath_close", "close", njpath_close),
    ("njpath_parallel_find", "parallel_find", njpath_parallel_find),
    ("njpath_parallel_find_one", "parallel_find_one", njpath_parallel_find_one),
    ("njpath_parallel_jmes", "parallel_jmes", njpath_parallel_jmes),
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

pub const MODULE_NAME: &str = "njpath";
pub const MODULE_PATHS: &[&str] = &["njpath", "std/njpath"];

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

    #[test]
    fn pointer_get_doctest() {
        let doc = Value::Object({
            let mut m = HashMap::new();
            m.insert("a".into(), Value::Int(1).ref_cell());
            m
        })
        .ref_cell();
        let v = njpath_pointer_get(
            &[doc, Value::String("/a".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert_eq!(*v.borrow(), Value::Int(1));
    }

    #[test]
    fn find_doctest() {
        let mut inner = HashMap::new();
        inner.insert("b".into(), Value::Int(2).ref_cell());
        let doc = Value::Object({
            let mut m = HashMap::new();
            m.insert("a".into(), Value::Object(inner).ref_cell());
            m
        })
        .ref_cell();
        let v = njpath_find(
            &[doc, Value::String("$.a.b".into()).ref_cell()],
            span(),
        )
        .unwrap();
        match &*v.borrow() {
            Value::Array(items) => assert_eq!(items.len(), 1),
            other => panic!("expected array, got {other:?}"),
        }
    }
}
