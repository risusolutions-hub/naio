//! Native ngraphql standard library — GraphQL client (queries, variables,
//! fragments) + schema/server helpers (~gql, graphene, strawberry).
//!
//! Import with `import "ngraphql"` (or `import "std/ngraphql"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_bignum::BigInt;
use niao_errors::codes;
use niao_graphql::{
    execute, execute_doc, fragment_summary, gql, is_document, is_schema, list_fragments,
    list_operations, minify, operation_summary, parse_document, print_document, request,
    request_json, spread_fragments, validate, variable_names, Document, GqlError, Schema,
};
use serde_json::{Map, Value as JsonValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E4450: u32 = codes::E4100_NGRAPHQL_ARITY;
const E4451: u32 = codes::E4101_NGRAPHQL_ERROR;
const E4452: u32 = codes::E4102_NGRAPHQL_TYPE;
const E4453: u32 = codes::E4103_NGRAPHQL_PARSE;
const E4454: u32 = codes::E4104_NGRAPHQL_INVALID_HANDLE;

thread_local! {
    static DOC_STORE: RefCell<HashMap<i64, Document>> = RefCell::new(HashMap::new());
    static SCHEMA_STORE: RefCell<HashMap<i64, Schema>> = RefCell::new(HashMap::new());
    static NEXT_DOC: RefCell<i64> = const { RefCell::new(1) };
    static NEXT_SCHEMA: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_doc(doc: Document) -> i64 {
    let id = NEXT_DOC.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    DOC_STORE.with(|m| m.borrow_mut().insert(id, doc));
    id
}

fn alloc_schema(schema: Schema) -> i64 {
    let id = NEXT_SCHEMA.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    SCHEMA_STORE.with(|m| m.borrow_mut().insert(id, schema));
    id
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4452, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4450,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4450,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn gql_err(span: Span, err: GqlError) -> ValueRef {
    let code = if err.message.starts_with("parse error") {
        E4453
    } else {
        E4451
    };
    error_value(code, "ngraphql_error", err.to_string(), span)
}

fn gql_err_msg(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4451, "ngraphql_error", msg.into(), span)
}

fn invalid_doc(span: Span, id: i64) -> ValueRef {
    error_value(
        E4454,
        "ngraphql_error",
        format!("invalid or closed ngraphql document handle {id}"),
        span,
    )
}

fn invalid_schema(span: Span, id: i64) -> ValueRef {
    error_value(
        E4454,
        "ngraphql_error",
        format!("invalid or closed ngraphql schema handle {id}"),
        span,
    )
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
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a handle (int) as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_string(args: &[ValueRef], idx: usize) -> Option<String> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn with_doc<T>(id: i64, span: Span, f: impl FnOnce(&Document) -> T) -> NiaoResult<Result<T, ValueRef>> {
    DOC_STORE.with(|m| match m.borrow().get(&id) {
        Some(d) => Ok(Ok(f(d))),
        None => Ok(Err(invalid_doc(span, id))),
    })
}

fn with_schema<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&Schema) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    SCHEMA_STORE.with(|m| match m.borrow().get(&id) {
        Some(s) => Ok(Ok(f(s))),
        None => Ok(Err(invalid_schema(span, id))),
    })
}

fn str_val(s: impl Into<String>) -> NiaoResult<ValueRef> {
    Ok(Value::String(s.into()).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

// ---------------------------------------------------------------------------
// JSON ↔ Niao Value (serde_json)
// ---------------------------------------------------------------------------

fn json_to_value(j: JsonValue) -> Value {
    match j {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(b) => Value::Bool(b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(u) = n.as_u64() {
                if u <= i64::MAX as u64 {
                    Value::Int(u as i64)
                } else {
                    Value::BigInt(BigInt::from(u))
                }
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Nil
            }
        }
        JsonValue::String(s) => Value::String(s),
        JsonValue::Array(items) => {
            Value::Array(items.into_iter().map(|i| json_to_value(i).ref_cell()).collect())
        }
        JsonValue::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k, json_to_value(v).ref_cell());
            }
            Value::Object(out)
        }
    }
}

fn value_to_json(v: &Value, span: Span) -> NiaoResult<JsonValue> {
    match v {
        Value::Nil => Ok(JsonValue::Null),
        Value::Bool(b) => Ok(JsonValue::Bool(*b)),
        Value::Int(n) => Ok(JsonValue::Number((*n).into())),
        Value::BigInt(n) => {
            if let Some(i) = n.to_i64() {
                Ok(JsonValue::Number(i.into()))
            } else if let Some(u) = n.to_u64() {
                Ok(JsonValue::Number(u.into()))
            } else {
                Err(type_err(
                    span,
                    format!("ngraphql: bigint {n} does not fit in JSON number"),
                ))
            }
        }
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(JsonValue::Number)
            .ok_or_else(|| type_err(span, "ngraphql: non-finite float")),
        Value::String(s) => Ok(JsonValue::String(s.clone())),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(value_to_json(&item.borrow(), span)?);
            }
            Ok(JsonValue::Array(out))
        }
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                out.insert(k.clone(), value_to_json(&v.borrow(), span)?);
            }
            Ok(JsonValue::Object(out))
        }
        other => Err(type_err(
            span,
            format!(
                "ngraphql: cannot convert {} to JSON",
                other.type_name()
            ),
        )),
    }
}

fn optional_json_object(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<Map<String, JsonValue>> {
    if args.len() <= idx {
        return Ok(Map::new());
    }
    match &*args[idx].borrow() {
        Value::Nil => Ok(Map::new()),
        Value::Object(_) | Value::Array(_) | Value::String(_) | Value::Bool(_) | Value::Int(_) | Value::Float(_) => {
            match value_to_json(&args[idx].borrow(), span)? {
                JsonValue::Object(m) => Ok(m),
                JsonValue::Null => Ok(Map::new()),
                _ => Err(type_err(span, "variables must be an object")),
            }
        }
        other => Err(type_err(
            span,
            format!("variables must be an object, got {}", other.type_name()),
        )),
    }
}

fn optional_root(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<JsonValue> {
    if args.len() <= idx {
        return Ok(JsonValue::Object(Map::new()));
    }
    match &*args[idx].borrow() {
        Value::Nil => Ok(JsonValue::Object(Map::new())),
        _ => value_to_json(&args[idx].borrow(), span),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> ngraphql.is_document("{ hero { name } }")
// => true
fn ngraphql_is_document(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_is_document", span)?;
    let src = string_arg(args, 0, "ngraphql_is_document", span)?;
    bool_val(is_document(&src))
}

// >>> ngraphql.is_schema("type Query { hello: String }")
// => true
fn ngraphql_is_schema(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_is_schema", span)?;
    let src = string_arg(args, 0, "ngraphql_is_schema", span)?;
    bool_val(is_schema(&src))
}

// >>> let d = ngraphql.parse("{ hero { name } }"); ngraphql.close_doc(d); true
// => true
fn ngraphql_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_parse", span)?;
    let src = string_arg(args, 0, "ngraphql_parse", span)?;
    match parse_document(&src) {
        Ok(doc) => int_val(alloc_doc(doc)),
        Err(e) => Ok(gql_err(span, e)),
    }
}

// >>> let d = ngraphql.parse("{ hero { name } }"); let s = ngraphql.print(d); ngraphql.close_doc(d); s.contains("hero")
// => true
fn ngraphql_print(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_print", span)?;
    let id = handle_arg(args, 0, "ngraphql_print", span)?;
    match with_doc(id, span, |d| print_document(d))? {
        Ok(s) => str_val(s),
        Err(e) => Ok(e),
    }
}

// >>> ngraphql.minify("{  hero  {  name  }  }").contains("hero")
// => true
fn ngraphql_minify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_minify", span)?;
    let src = string_arg(args, 0, "ngraphql_minify", span)?;
    match minify(&src) {
        Ok(s) => str_val(s),
        Err(e) => Ok(gql_err(span, e)),
    }
}

// >>> ngraphql.gql("{ hero { name } }").contains("hero")
// => true
fn ngraphql_gql(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_gql", span)?;
    let src = string_arg(args, 0, "ngraphql_gql", span)?;
    match gql(&src) {
        Ok(s) => str_val(s),
        Err(e) => Ok(gql_err(span, e)),
    }
}

// >>> let d = ngraphql.parse("{ x }"); ngraphql.close_doc(d); true
// => true
fn ngraphql_close_doc(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_close_doc", span)?;
    let id = handle_arg(args, 0, "ngraphql_close_doc", span)?;
    let removed = DOC_STORE.with(|m| m.borrow_mut().remove(&id).is_some());
    if removed {
        bool_val(true)
    } else {
        Ok(invalid_doc(span, id))
    }
}

// >>> let d = ngraphql.parse("query Q { x }"); let ops = ngraphql.operations(d); ngraphql.close_doc(d); len(ops) == 1
// => true
fn ngraphql_operations(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_operations", span)?;
    let id = handle_arg(args, 0, "ngraphql_operations", span)?;
    match with_doc(id, span, |d| {
        list_operations(d)
            .into_iter()
            .map(|op| json_to_value(operation_summary(op)).ref_cell())
            .collect::<Vec<_>>()
    })? {
        Ok(arr) => Ok(Value::Array(arr).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> let d = ngraphql.parse("fragment F on T { x } query { ...F }"); let f = ngraphql.fragments(d); ngraphql.close_doc(d); len(f) == 1
// => true
fn ngraphql_fragments(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_fragments", span)?;
    let id = handle_arg(args, 0, "ngraphql_fragments", span)?;
    match with_doc(id, span, |d| {
        list_fragments(d)
            .into_iter()
            .map(|f| json_to_value(fragment_summary(f)).ref_cell())
            .collect::<Vec<_>>()
    })? {
        Ok(arr) => Ok(Value::Array(arr).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> let d = ngraphql.parse("query A { x } query B { y }"); let n = ngraphql.operation_names(d); ngraphql.close_doc(d); len(n) == 2
// => true
fn ngraphql_operation_names(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_operation_names", span)?;
    let id = handle_arg(args, 0, "ngraphql_operation_names", span)?;
    match with_doc(id, span, |d| {
        list_operations(d)
            .into_iter()
            .filter_map(|o| o.name.clone())
            .map(|n| Value::String(n).ref_cell())
            .collect::<Vec<_>>()
    })? {
        Ok(arr) => Ok(Value::Array(arr).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> let d = ngraphql.parse("query A { x }"); let b = ngraphql.has_operation(d, "A"); ngraphql.close_doc(d); b
// => true
fn ngraphql_has_operation(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngraphql_has_operation", span)?;
    let id = handle_arg(args, 0, "ngraphql_has_operation", span)?;
    let name = string_arg(args, 1, "ngraphql_has_operation", span)?;
    match with_doc(id, span, |d| {
        list_operations(d)
            .iter()
            .any(|o| o.name.as_deref() == Some(name.as_str()))
    })? {
        Ok(b) => bool_val(b),
        Err(e) => Ok(e),
    }
}

// >>> let d = ngraphql.parse("query Q($id: ID!) { x }"); let op = ngraphql.get_operation(d, "Q"); ngraphql.close_doc(d); op.kind == "query"
// => true
fn ngraphql_get_operation(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ngraphql_get_operation", span)?;
    let id = handle_arg(args, 0, "ngraphql_get_operation", span)?;
    let name = optional_string(args, 1);
    match with_doc(id, span, |d| -> Result<ValueRef, ValueRef> {
        let ops = list_operations(d);
        let op = if let Some(n) = &name {
            ops.iter()
                .find(|o| o.name.as_deref() == Some(n.as_str()))
                .copied()
        } else if ops.len() == 1 {
            Some(ops[0])
        } else {
            None
        };
        match op {
            Some(op) => Ok(json_to_value(operation_summary(op)).ref_cell()),
            None => Err(gql_err_msg(span, "operation not found")),
        }
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

// >>> let d = ngraphql.parse("query($id: ID!) { x }"); let v = ngraphql.variable_names(d); ngraphql.close_doc(d); v[0] == "id"
// => true
fn ngraphql_variable_names(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ngraphql_variable_names", span)?;
    let id = handle_arg(args, 0, "ngraphql_variable_names", span)?;
    let op = optional_string(args, 1);
    match with_doc(id, span, |d| variable_names(d, op.as_deref()))? {
        Ok(Ok(names)) => Ok(Value::Array(
            names
                .into_iter()
                .map(|n| Value::String(n).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Ok(Err(e)) => Ok(gql_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> let d = ngraphql.parse("fragment F on T { a } query { ...F }"); let s = ngraphql.spread_fragments(d); ngraphql.close_doc(d); ngraphql.close_doc(s); true
// => true
fn ngraphql_spread_fragments(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_spread_fragments", span)?;
    let id = handle_arg(args, 0, "ngraphql_spread_fragments", span)?;
    match with_doc(id, span, |d| spread_fragments(d))? {
        Ok(Ok(doc)) => int_val(alloc_doc(doc)),
        Ok(Err(e)) => Ok(gql_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> let r = ngraphql.request("{ hello }", {id: 1}); r.query.contains("hello")
// => true
fn ngraphql_request(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "ngraphql_request", span)?;
    let query = string_arg(args, 0, "ngraphql_request", span)?;
    let vars = optional_json_object(args, 1, span)?;
    let op = optional_string(args, 2);
    let vars_ref = if vars.is_empty() { None } else { Some(&vars) };
    match request(&query, vars_ref, op.as_deref()) {
        Ok(j) => Ok(json_to_value(j).ref_cell()),
        Err(e) => Ok(gql_err(span, e)),
    }
}

// >>> let s = ngraphql.request_json("{ hello }"); s.contains("query")
// => true
fn ngraphql_request_json(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "ngraphql_request_json", span)?;
    let query = string_arg(args, 0, "ngraphql_request_json", span)?;
    let vars = optional_json_object(args, 1, span)?;
    let op = optional_string(args, 2);
    let vars_ref = if vars.is_empty() { None } else { Some(&vars) };
    match request_json(&query, vars_ref, op.as_deref()) {
        Ok(s) => str_val(s),
        Err(e) => Ok(gql_err(span, e)),
    }
}

// >>> let s = ngraphql.parse_schema("type Query { hello: String }"); ngraphql.close_schema(s); true
// => true
fn ngraphql_parse_schema(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_parse_schema", span)?;
    let src = string_arg(args, 0, "ngraphql_parse_schema", span)?;
    match Schema::parse(&src) {
        Ok(schema) => int_val(alloc_schema(schema)),
        Err(e) => Ok(gql_err(span, e)),
    }
}

// >>> let s = ngraphql.parse_schema("type Query { hello: String }"); let t = ngraphql.print_schema(s); ngraphql.close_schema(s); t.contains("Query")
// => true
fn ngraphql_print_schema(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_print_schema", span)?;
    let id = handle_arg(args, 0, "ngraphql_print_schema", span)?;
    match with_schema(id, span, |s| s.print())? {
        Ok(t) => str_val(t),
        Err(e) => Ok(e),
    }
}

// >>> let s = ngraphql.parse_schema("type Query { hello: String }"); ngraphql.close_schema(s); true
// => true
fn ngraphql_close_schema(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_close_schema", span)?;
    let id = handle_arg(args, 0, "ngraphql_close_schema", span)?;
    let removed = SCHEMA_STORE.with(|m| m.borrow_mut().remove(&id).is_some());
    if removed {
        bool_val(true)
    } else {
        Ok(invalid_schema(span, id))
    }
}

// >>> let s = ngraphql.parse_schema("type Query { hello: String }"); let n = ngraphql.type_names(s); ngraphql.close_schema(s); n.contains("Query")
// => true
fn ngraphql_type_names(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_type_names", span)?;
    let id = handle_arg(args, 0, "ngraphql_type_names", span)?;
    match with_schema(id, span, |s| {
        s.type_names()
            .into_iter()
            .map(|n| Value::String(n).ref_cell())
            .collect::<Vec<_>>()
    })? {
        Ok(arr) => Ok(Value::Array(arr).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> let s = ngraphql.parse_schema("type Query { hello: String! }"); let d = ngraphql.describe_type(s, "Query"); ngraphql.close_schema(s); d.kind == "OBJECT"
// => true
fn ngraphql_describe_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngraphql_describe_type", span)?;
    let id = handle_arg(args, 0, "ngraphql_describe_type", span)?;
    let name = string_arg(args, 1, "ngraphql_describe_type", span)?;
    match with_schema(id, span, |s| s.describe_type(&name))? {
        Ok(Ok(j)) => Ok(json_to_value(j).ref_cell()),
        Ok(Err(e)) => Ok(gql_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> let s = ngraphql.parse_schema("type Query { hello: String }"); let q = ngraphql.query_type(s); ngraphql.close_schema(s); q == "Query"
// => true
fn ngraphql_query_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_query_type", span)?;
    let id = handle_arg(args, 0, "ngraphql_query_type", span)?;
    match with_schema(id, span, |s| s.query_type.clone())? {
        Ok(q) => str_val(q),
        Err(e) => Ok(e),
    }
}

// >>> let s = ngraphql.parse_schema("type Query { x: String } type Mutation { y: String }"); let m = ngraphql.mutation_type(s); ngraphql.close_schema(s); m == "Mutation"
// => true
fn ngraphql_mutation_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_mutation_type", span)?;
    let id = handle_arg(args, 0, "ngraphql_mutation_type", span)?;
    match with_schema(id, span, |s| s.mutation_type.clone())? {
        Ok(Some(m)) => str_val(m),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> let s = ngraphql.parse_schema("type Query { x: String }"); let sub = ngraphql.subscription_type(s); ngraphql.close_schema(s); sub == nil
// => true
fn ngraphql_subscription_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngraphql_subscription_type", span)?;
    let id = handle_arg(args, 0, "ngraphql_subscription_type", span)?;
    match with_schema(id, span, |s| s.subscription_type.clone())? {
        Ok(Some(m)) => str_val(m),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> let s = ngraphql.parse_schema("type Query { hello: String }"); let b = ngraphql.has_type(s, "Query"); ngraphql.close_schema(s); b
// => true
fn ngraphql_has_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngraphql_has_type", span)?;
    let id = handle_arg(args, 0, "ngraphql_has_type", span)?;
    let name = string_arg(args, 1, "ngraphql_has_type", span)?;
    match with_schema(id, span, |s| s.has_type(&name))? {
        Ok(b) => bool_val(b),
        Err(e) => Ok(e),
    }
}

// >>> let s = ngraphql.parse_schema("type Query { hello: String }"); let d = ngraphql.parse("{ hello }"); let v = ngraphql.validate(d, s); ngraphql.close_doc(d); ngraphql.close_schema(s); v.ok
// => true
fn ngraphql_validate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ngraphql_validate", span)?;
    let schema_id = handle_arg(args, 1, "ngraphql_validate", span)?;
    let op = optional_string(args, 2);

    let doc = match &*args[0].borrow() {
        Value::String(src) => match parse_document(src) {
            Ok(d) => d,
            Err(e) => return Ok(gql_err(span, e)),
        },
        Value::Int(id) => match with_doc(*id, span, |d| d.clone())? {
            Ok(d) => d,
            Err(e) => return Ok(e),
        },
        other => {
            return Err(type_err(
                span,
                format!(
                    "ngraphql_validate() expects document handle or query string, got {}",
                    other.type_name()
                ),
            ))
        }
    };

    match with_schema(schema_id, span, |s| validate(&doc, s, op.as_deref()))? {
        Ok(Ok(v)) => Ok(json_to_value(v.to_json()).ref_cell()),
        Ok(Err(e)) => Ok(gql_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> let s = ngraphql.parse_schema("type Query { hello: String }"); let r = ngraphql.execute(s, "{ hello }", {hello: "hi"}); ngraphql.close_schema(s); r.data.hello == "hi"
// => true
fn ngraphql_execute(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 5, "ngraphql_execute", span)?;
    let schema_id = handle_arg(args, 0, "ngraphql_execute", span)?;
    let query = string_arg(args, 1, "ngraphql_execute", span)?;
    let root = optional_root(args, 2, span)?;
    let vars = optional_json_object(args, 3, span)?;
    let op = optional_string(args, 4);
    match with_schema(schema_id, span, |s| {
        execute(s, &query, &root, &vars, op.as_deref())
    })? {
        Ok(Ok(r)) => Ok(json_to_value(r.to_json()).ref_cell()),
        Ok(Err(e)) => Ok(gql_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> let s = ngraphql.parse_schema("type Query { hello: String }"); let d = ngraphql.parse("{ hello }"); let r = ngraphql.execute_doc(s, d, {hello: "hi"}); ngraphql.close_doc(d); ngraphql.close_schema(s); r.data.hello == "hi"
// => true
fn ngraphql_execute_doc(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 5, "ngraphql_execute_doc", span)?;
    let schema_id = handle_arg(args, 0, "ngraphql_execute_doc", span)?;
    let doc_id = handle_arg(args, 1, "ngraphql_execute_doc", span)?;
    let root = optional_root(args, 2, span)?;
    let vars = optional_json_object(args, 3, span)?;
    let op = optional_string(args, 4);
    let doc = match with_doc(doc_id, span, |d| d.clone())? {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    match with_schema(schema_id, span, |s| {
        execute_doc(s, &doc, &root, &vars, op.as_deref())
    })? {
        Ok(Ok(r)) => Ok(json_to_value(r.to_json()).ref_cell()),
        Ok(Err(e)) => Ok(gql_err(span, e)),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ngraphql_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ngraphql_fns![
    ("ngraphql_parse", "parse", ngraphql_parse),
    ("ngraphql_print", "print", ngraphql_print),
    ("ngraphql_minify", "minify", ngraphql_minify),
    ("ngraphql_gql", "gql", ngraphql_gql),
    ("ngraphql_close_doc", "close_doc", ngraphql_close_doc),
    ("ngraphql_operations", "operations", ngraphql_operations),
    ("ngraphql_fragments", "fragments", ngraphql_fragments),
    ("ngraphql_operation_names", "operation_names", ngraphql_operation_names),
    ("ngraphql_has_operation", "has_operation", ngraphql_has_operation),
    ("ngraphql_get_operation", "get_operation", ngraphql_get_operation),
    ("ngraphql_variable_names", "variable_names", ngraphql_variable_names),
    ("ngraphql_spread_fragments", "spread_fragments", ngraphql_spread_fragments),
    ("ngraphql_request", "request", ngraphql_request),
    ("ngraphql_request_json", "request_json", ngraphql_request_json),
    ("ngraphql_parse_schema", "parse_schema", ngraphql_parse_schema),
    ("ngraphql_print_schema", "print_schema", ngraphql_print_schema),
    ("ngraphql_close_schema", "close_schema", ngraphql_close_schema),
    ("ngraphql_type_names", "type_names", ngraphql_type_names),
    ("ngraphql_describe_type", "describe_type", ngraphql_describe_type),
    ("ngraphql_query_type", "query_type", ngraphql_query_type),
    ("ngraphql_mutation_type", "mutation_type", ngraphql_mutation_type),
    ("ngraphql_subscription_type", "subscription_type", ngraphql_subscription_type),
    ("ngraphql_has_type", "has_type", ngraphql_has_type),
    ("ngraphql_validate", "validate", ngraphql_validate),
    ("ngraphql_execute", "execute", ngraphql_execute),
    ("ngraphql_execute_doc", "execute_doc", ngraphql_execute_doc),
    ("ngraphql_is_document", "is_document", ngraphql_is_document),
    ("ngraphql_is_schema", "is_schema", ngraphql_is_schema),
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

pub const MODULE_NAME: &str = "ngraphql";
pub const MODULE_PATHS: &[&str] = &["ngraphql", "std/ngraphql"];

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
    fn parse_print_roundtrip() {
        let args = [Value::String("{ hero { name } }".into()).ref_cell()];
        let doc = ngraphql_parse(&args, span()).unwrap();
        let printed = ngraphql_print(&[doc.clone()], span()).unwrap();
        match &*printed.borrow() {
            Value::String(s) => assert!(s.contains("hero")),
            other => panic!("expected string, got {other:?}"),
        }
        let _ = ngraphql_close_doc(&[doc], span());
    }

    #[test]
    fn execute_hello() {
        let sdl = Value::String("type Query { hello: String }".into()).ref_cell();
        let schema = ngraphql_parse_schema(&[sdl], span()).unwrap();
        let mut root = HashMap::new();
        root.insert("hello".into(), Value::String("world".into()).ref_cell());
        let args = [
            schema.clone(),
            Value::String("{ hello }".into()).ref_cell(),
            Value::Object(root).ref_cell(),
        ];
        let result = ngraphql_execute(&args, span()).unwrap();
        match &*result.borrow() {
            Value::Object(m) => {
                let data = m.get("data").unwrap().borrow();
                match &*data {
                    Value::Object(d) => match &*d.get("hello").unwrap().borrow() {
                        Value::String(s) => assert_eq!(s, "world"),
                        other => panic!("{other:?}"),
                    },
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
        let _ = ngraphql_close_schema(&[schema], span());
    }
}
