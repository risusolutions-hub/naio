//! Native nsearch standard library — hosted search clients
//! (~elasticsearch, meilisearch).
//!
//! Import with `import "nsearch"` (or `import "std/nsearch"`).

use crate::{error_value, json_stringify, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_json_core::Value as JsonValue;
use niao_search::{
    bulk as search_bulk, create_index, delete_doc, delete_index, encode_params, es_bulk_ndjson,
    es_query, extract_hits, get_doc, index_doc, index_exists, join_url, list_indexes, meili_filter,
    raw_request, search as do_search, ts_filter, update_doc, Auth, BulkOp, Client, Engine,
    EsQueryOpts, HttpResponse, SearchError, SearchOpts,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E4550: u32 = codes::E4550_NSEARCH_ARITY;
const E4551: u32 = codes::E4551_NSEARCH_ERROR;
const E4552: u32 = codes::E4552_NSEARCH_TYPE;
const E4553: u32 = codes::E4553_NSEARCH_INVALID_HANDLE;

thread_local! {
    static CLIENTS: RefCell<HashMap<i64, Client>> = RefCell::new(HashMap::new());
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
    RuntimeError::at(span, E4552, msg.into())
}

fn search_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4551, "nsearch_error", msg.into(), span)
}

fn map_err(span: Span, e: SearchError) -> ValueRef {
    search_err(span, e.to_string())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4550,
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
                "{name}() expects string as argument {}, got {}",
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
                "{name}() expects positive client handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(m) => Some(m.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn int_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<i64> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => Some(n),
        _ => None,
    }
}

fn string_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<String> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    }
}

fn string_map_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Vec<(String, String)> {
    let Some(map) = map else {
        return Vec::new();
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Object(obj)) => {
            let mut out = Vec::new();
            for (k, v) in obj {
                match &*v.borrow() {
                    Value::String(s) => out.push((k, s.clone())),
                    Value::Int(n) => out.push((k, n.to_string())),
                    Value::Float(f) => out.push((k, f.to_string())),
                    Value::Bool(b) => out.push((k, b.to_string())),
                    _ => {}
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

fn value_to_json_string(v: &Value, span: Span) -> NiaoResult<String> {
    let vr = v.clone().ref_cell();
    let out = json_stringify(&[vr], span)?;
    let s = match &*out.borrow() {
        Value::String(s) => s.clone(),
        _ => return Err(type_err(span, "json stringify did not return string")),
    };
    Ok(s)
}

fn with_client<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&Client) -> NiaoResult<ValueRef>,
{
    CLIENTS.with(|m| {
        let map = m.borrow();
        match map.get(&id) {
            Some(c) => f(c),
            None => Ok(error_value(
                E4553,
                "nsearch_error",
                format!("invalid client handle: {id}"),
                span,
            )),
        }
    })
}

fn http_to_value(resp: HttpResponse) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("status".into(), Value::Int(resp.status as i64).ref_cell());
    map.insert("ok".into(), Value::Bool(resp.ok).ref_cell());
    map.insert("url".into(), Value::String(resp.url).ref_cell());
    map.insert("body".into(), Value::String(resp.body).ref_cell());
    map.insert(
        "elapsed_ms".into(),
        Value::Int(resp.elapsed_ms as i64).ref_cell(),
    );
    Value::Object(map).ref_cell()
}

fn json_to_niao(v: &JsonValue) -> Value {
    match v {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Float(0.0)
            }
        }
        JsonValue::String(s) => Value::String(s.clone()),
        JsonValue::Array(items) => {
            Value::Array(items.iter().map(|i| json_to_niao(i).ref_cell()).collect())
        }
        JsonValue::Object(o) => {
            let mut map = HashMap::new();
            for (k, v) in o.iter() {
                map.insert(k.to_string(), json_to_niao(v).ref_cell());
            }
            Value::Object(map)
        }
    }
}

fn parse_client_opts(
    map: Option<&HashMap<String, ValueRef>>,
    engine: Engine,
    span: Span,
) -> NiaoResult<Client> {
    let url = string_field(map, "url");
    let cloud_id = string_field(map, "cloud_id");
    let api_key = string_field(map, "api_key");
    let key = string_field(map, "key");
    let bearer = string_field(map, "bearer");
    let username = string_field(map, "username").or_else(|| string_field(map, "user"));
    let password = string_field(map, "password").or_else(|| string_field(map, "pass"));
    let timeout_ms = int_field(map, "timeout_ms").map(|n| n.max(0) as u64);
    niao_search::build_client(
        engine, url, cloud_id, api_key, username, password, bearer, key, timeout_ms,
    )
    .map_err(|e| type_err(span, e.to_string()))
}

fn make_client(engine: Engine, args: &[ValueRef], name: &str, span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, name, span)?;
    let client = parse_client_opts(optional_object(args, 0).as_ref(), engine, span)?;
    let id = new_id();
    CLIENTS.with(|m| m.borrow_mut().insert(id, client));
    Ok(Value::Int(id).ref_cell())
}

fn parse_search_opts(
    map: Option<&HashMap<String, ValueRef>>,
    span: Span,
) -> NiaoResult<SearchOpts> {
    let Some(map) = map else {
        return Err(type_err(span, "search() requires options object"));
    };
    let index = string_field(Some(map), "index")
        .or_else(|| string_field(Some(map), "collection"))
        .or_else(|| string_field(Some(map), "uid"))
        .unwrap_or_default();
    let mut opts = SearchOpts {
        index,
        q: string_field(Some(map), "q").or_else(|| string_field(Some(map), "query")),
        query_by: string_field(Some(map), "query_by"),
        filter: string_field(Some(map), "filter").or_else(|| string_field(Some(map), "filter_by")),
        sort: string_field(Some(map), "sort").or_else(|| string_field(Some(map), "sort_by")),
        limit: int_field(Some(map), "limit").or_else(|| int_field(Some(map), "size")),
        offset: int_field(Some(map), "offset").or_else(|| int_field(Some(map), "from")),
        body: None,
        fields: Vec::new(),
    };
    if let Some(v) = map.get("fields") {
        match &*v.borrow() {
            Value::Array(items) => {
                for it in items {
                    if let Value::String(s) = &*it.borrow() {
                        opts.fields.push(s.clone());
                    }
                }
            }
            Value::String(s) => {
                opts.fields = s.split(',').map(|p| p.trim().to_string()).collect();
            }
            _ => {}
        }
    }
    if let Some(v) = map.get("body").or_else(|| map.get("json")) {
        opts.body = Some(value_to_json_string(&v.borrow(), span)?);
    }
    Ok(opts)
}

fn doc_json_arg(v: &Value, span: Span) -> NiaoResult<String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Object(_) | Value::Array(_) => value_to_json_string(v, span),
        other => Err(type_err(
            span,
            format!("document must be object/string, got {}", other.type_name()),
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nsearch_elasticsearch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> let c = nsearch.elasticsearch({url: "http://localhost:9200"}); nsearch.close(c)
    // => true
    make_client(Engine::Elasticsearch, args, "nsearch_elasticsearch", span)
}

fn nsearch_opensearch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> let c = nsearch.opensearch({url: "http://localhost:9200"}); nsearch.close(c)
    // => true
    make_client(Engine::OpenSearch, args, "nsearch_opensearch", span)
}

fn nsearch_meilisearch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> let c = nsearch.meilisearch({url: "http://localhost:7700", key: "k"}); nsearch.close(c)
    // => true
    make_client(Engine::Meilisearch, args, "nsearch_meilisearch", span)
}

fn nsearch_typesense(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> let c = nsearch.typesense({url: "http://localhost:8108", api_key: "k"}); nsearch.close(c)
    // => true
    make_client(Engine::Typesense, args, "nsearch_typesense", span)
}

fn nsearch_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> let c = nsearch.elasticsearch({url: "http://x"}); nsearch.close(c)
    // => true
    arity_range(args, 1, 1, "nsearch_close", span)?;
    let id = handle_arg(args, 0, "nsearch_close", span)?;
    let removed = CLIENTS.with(|m| m.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

fn nsearch_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> let c = nsearch.elasticsearch({url: "http://localhost:9200"}); let i = nsearch.info(c); nsearch.close(c); i.engine
    // => "elasticsearch"
    arity_range(args, 1, 1, "nsearch_info", span)?;
    let id = handle_arg(args, 0, "nsearch_info", span)?;
    with_client(id, span, |c| {
        let mut map = HashMap::new();
        map.insert(
            "engine".into(),
            Value::String(c.engine.as_str().into()).ref_cell(),
        );
        map.insert("url".into(), Value::String(c.base_url.clone()).ref_cell());
        map.insert(
            "timeout_ms".into(),
            Value::Int(c.timeout_ms as i64).ref_cell(),
        );
        let auth = match &c.auth {
            Auth::None => "none",
            Auth::ApiKey(_) => "api_key",
            Auth::Basic { .. } => "basic",
            Auth::Bearer(_) => "bearer",
        };
        map.insert("auth".into(), Value::String(auth.into()).ref_cell());
        Ok(Value::Object(map).ref_cell())
    })
}

fn nsearch_engine(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> let c = nsearch.meilisearch({url: "http://localhost:7700"}); let e = nsearch.engine(c); nsearch.close(c); e
    // => "meilisearch"
    arity_range(args, 1, 1, "nsearch_engine", span)?;
    let id = handle_arg(args, 0, "nsearch_engine", span)?;
    with_client(id, span, |c| {
        Ok(Value::String(c.engine.as_str().into()).ref_cell())
    })
}

fn nsearch_search(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nsearch_search", span)?;
    let id = handle_arg(args, 0, "nsearch_search", span)?;
    let opts = parse_search_opts(optional_object(args, 1).as_ref(), span)?;
    with_client(id, span, |c| match do_search(c, &opts) {
        Ok(r) => Ok(http_to_value(r)),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nsearch_index(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nsearch_index", span)?;
    let id = handle_arg(args, 0, "nsearch_index", span)?;
    let index = string_arg(args, 1, "nsearch_index", span)?;
    let doc = doc_json_arg(&args[2].borrow(), span)?;
    let doc_id = if args.len() >= 4 {
        match &*args[3].borrow() {
            Value::Object(m) => string_field(Some(m), "id"),
            Value::String(s) => Some(s.clone()),
            Value::Nil => None,
            _ => None,
        }
    } else {
        None
    };
    with_client(id, span, |c| match index_doc(c, &index, &doc, doc_id.as_deref()) {
        Ok(r) => Ok(http_to_value(r)),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nsearch_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 3, "nsearch_get", span)?;
    let id = handle_arg(args, 0, "nsearch_get", span)?;
    let index = string_arg(args, 1, "nsearch_get", span)?;
    let doc_id = string_arg(args, 2, "nsearch_get", span)?;
    with_client(id, span, |c| match get_doc(c, &index, &doc_id) {
        Ok(r) => Ok(http_to_value(r)),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nsearch_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 3, "nsearch_delete", span)?;
    let id = handle_arg(args, 0, "nsearch_delete", span)?;
    let index = string_arg(args, 1, "nsearch_delete", span)?;
    let doc_id = string_arg(args, 2, "nsearch_delete", span)?;
    with_client(id, span, |c| match delete_doc(c, &index, &doc_id) {
        Ok(r) => Ok(http_to_value(r)),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nsearch_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 4, 4, "nsearch_update", span)?;
    let id = handle_arg(args, 0, "nsearch_update", span)?;
    let index = string_arg(args, 1, "nsearch_update", span)?;
    let doc_id = string_arg(args, 2, "nsearch_update", span)?;
    let doc = doc_json_arg(&args[3].borrow(), span)?;
    with_client(id, span, |c| match update_doc(c, &index, &doc_id, &doc) {
        Ok(r) => Ok(http_to_value(r)),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn parse_bulk_ops(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<Vec<BulkOp>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut ops = Vec::new();
            for it in items {
                match &*it.borrow() {
                    Value::Object(m) => {
                        let action = string_field(Some(m), "action")
                            .or_else(|| string_field(Some(m), "op"))
                            .unwrap_or_else(|| "index".into());
                        let index = string_field(Some(m), "index")
                            .or_else(|| string_field(Some(m), "collection"))
                            .unwrap_or_default();
                        let id = string_field(Some(m), "id");
                        let doc_json = if let Some(v) = m.get("doc").or_else(|| m.get("body")) {
                            Some(doc_json_arg(&v.borrow(), span)?)
                        } else {
                            None
                        };
                        ops.push(BulkOp {
                            action,
                            index,
                            id,
                            doc_json,
                        });
                    }
                    _ => {
                        return Err(type_err(span, "bulk ops must be objects"));
                    }
                }
            }
            Ok(ops)
        }
        _ => Err(type_err(span, "bulk() expects array of ops")),
    }
}

fn nsearch_bulk(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nsearch_bulk", span)?;
    let id = handle_arg(args, 0, "nsearch_bulk", span)?;
    let ops = parse_bulk_ops(args, 1, span)?;
    with_client(id, span, |c| match search_bulk(c, &ops) {
        Ok(r) => Ok(http_to_value(r)),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nsearch_create_index(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsearch_create_index", span)?;
    let id = handle_arg(args, 0, "nsearch_create_index", span)?;
    let name = string_arg(args, 1, "nsearch_create_index", span)?;
    let settings = if args.len() >= 3 {
        Some(doc_json_arg(&args[2].borrow(), span)?)
    } else {
        None
    };
    with_client(id, span, |c| {
        match create_index(c, &name, settings.as_deref()) {
            Ok(r) => Ok(http_to_value(r)),
            Err(e) => Ok(map_err(span, e)),
        }
    })
}

fn nsearch_delete_index(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nsearch_delete_index", span)?;
    let id = handle_arg(args, 0, "nsearch_delete_index", span)?;
    let name = string_arg(args, 1, "nsearch_delete_index", span)?;
    with_client(id, span, |c| match delete_index(c, &name) {
        Ok(r) => Ok(http_to_value(r)),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nsearch_list_indexes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nsearch_list_indexes", span)?;
    let id = handle_arg(args, 0, "nsearch_list_indexes", span)?;
    with_client(id, span, |c| match list_indexes(c) {
        Ok(r) => Ok(http_to_value(r)),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nsearch_index_exists(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nsearch_index_exists", span)?;
    let id = handle_arg(args, 0, "nsearch_index_exists", span)?;
    let name = string_arg(args, 1, "nsearch_index_exists", span)?;
    with_client(id, span, |c| match index_exists(c, &name) {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nsearch_request(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nsearch_request", span)?;
    let id = handle_arg(args, 0, "nsearch_request", span)?;
    let method = string_arg(args, 1, "nsearch_request", span)?;
    let path = string_arg(args, 2, "nsearch_request", span)?;
    let (body, params) = if let Some(map) = optional_object(args, 3) {
        let body = if let Some(v) = map.get("body").or_else(|| map.get("json")) {
            Some(doc_json_arg(&v.borrow(), span)?)
        } else if let Some(s) = string_field(Some(&map), "body") {
            Some(s)
        } else {
            None
        };
        let params = string_map_field(Some(&map), "params");
        (body, params)
    } else {
        (None, Vec::new())
    };
    with_client(id, span, |c| match raw_request(c, &method, &path, body, params) {
        Ok(r) => Ok(http_to_value(r)),
        Err(e) => Ok(map_err(span, e)),
    })
}

fn nsearch_join(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nsearch.join("http://localhost:9200/", "_search")
    // => "http://localhost:9200/_search"
    arity_range(args, 2, 2, "nsearch_join", span)?;
    let base = string_arg(args, 0, "nsearch_join", span)?;
    let path = string_arg(args, 1, "nsearch_join", span)?;
    match join_url(&base, &path) {
        Ok(u) => Ok(Value::String(u).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nsearch_encode_params(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nsearch.encode_params({q: "a b", page: "1"}).contains("page=1")
    // => true
    arity_range(args, 1, 1, "nsearch_encode_params", span)?;
    let pairs = match &*args[0].borrow() {
        Value::Object(m) => {
            let mut out = Vec::new();
            for (k, v) in m {
                match &*v.borrow() {
                    Value::String(s) => out.push((k.clone(), s.clone())),
                    Value::Int(n) => out.push((k.clone(), n.to_string())),
                    Value::Float(f) => out.push((k.clone(), f.to_string())),
                    Value::Bool(b) => out.push((k.clone(), b.to_string())),
                    _ => {}
                }
            }
            out
        }
        _ => {
            return Err(type_err(
                span,
                "encode_params() expects object",
            ))
        }
    };
    Ok(Value::String(encode_params(&pairs)).ref_cell())
}

fn nsearch_es_query(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nsearch.es_query({q: "niao", size: 10}).contains("multi_match")
    // => true
    arity_range(args, 0, 1, "nsearch_es_query", span)?;
    let map = optional_object(args, 0);
    let mut opts = EsQueryOpts::default();
    if let Some(m) = map.as_ref() {
        opts.q = string_field(Some(m), "q").or_else(|| string_field(Some(m), "query"));
        opts.size = int_field(Some(m), "size").or_else(|| int_field(Some(m), "limit"));
        opts.from = int_field(Some(m), "from").or_else(|| int_field(Some(m), "offset"));
        opts.match_all = match m.get("match_all").map(|v| v.borrow().clone()) {
            Some(Value::Bool(b)) => b,
            Some(Value::Int(n)) => n != 0,
            _ => false,
        };
        if let Some(v) = m.get("body").or_else(|| m.get("json")) {
            opts.body = Some(value_to_json_string(&v.borrow(), span)?);
        }
        if let Some(v) = m.get("fields") {
            match &*v.borrow() {
                Value::Array(items) => {
                    for it in items {
                        if let Value::String(s) = &*it.borrow() {
                            opts.fields.push(s.clone());
                        }
                    }
                }
                Value::String(s) => {
                    opts.fields = s.split(',').map(|p| p.trim().to_string()).collect();
                }
                _ => {}
            }
        }
        if let Some(Value::Object(obj)) = m.get("match").map(|v| v.borrow().clone()) {
            for (k, v) in obj {
                if let Value::String(s) = &*v.borrow() {
                    opts.match_fields.push((k, s.clone()));
                }
            }
        }
        if let Some(Value::Object(obj)) = m.get("term").map(|v| v.borrow().clone()) {
            for (k, v) in obj {
                match &*v.borrow() {
                    Value::String(s) => opts.term_fields.push((k, s.clone())),
                    Value::Int(n) => opts.term_fields.push((k, n.to_string())),
                    _ => {}
                }
            }
        }
        if let Some(v) = m.get("sort") {
            opts.sort_json = Some(value_to_json_string(&v.borrow(), span)?);
        }
    } else {
        opts.match_all = true;
    }
    Ok(Value::String(es_query(&opts)).ref_cell())
}

fn nsearch_es_bulk_ndjson(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nsearch.es_bulk_ndjson([{action: "index", index: "d", id: "1", doc: {a: 1}}]).contains("_index")
    // => true
    arity_range(args, 1, 1, "nsearch_es_bulk_ndjson", span)?;
    let ops = parse_bulk_ops(args, 0, span)?;
    match es_bulk_ndjson(&ops) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn string_list_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<String>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::new();
            for it in items {
                match &*it.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!("{name}() expects string array, got {}", other.type_name()),
                        ))
                    }
                }
            }
            Ok(out)
        }
        Value::String(s) => Ok(vec![s.clone()]),
        other => Err(type_err(
            span,
            format!("{name}() expects array/string, got {}", other.type_name()),
        )),
    }
}

fn nsearch_meili_filter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nsearch.meili_filter(["a = 1", "b > 2"])
    // => "a = 1 AND b > 2"
    arity_range(args, 1, 1, "nsearch_meili_filter", span)?;
    let parts = string_list_arg(args, 0, "nsearch_meili_filter", span)?;
    Ok(Value::String(meili_filter(&parts)).ref_cell())
}

fn nsearch_ts_filter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nsearch.ts_filter(["year:>2000", "in_stock:true"])
    // => "year:>2000 && in_stock:true"
    arity_range(args, 1, 1, "nsearch_ts_filter", span)?;
    let parts = string_list_arg(args, 0, "nsearch_ts_filter", span)?;
    Ok(Value::String(ts_filter(&parts)).ref_cell())
}

fn nsearch_ok(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nsearch.ok({status: 200, ok: true, body: "", url: "", elapsed_ms: 0})
    // => true
    arity_range(args, 1, 1, "nsearch_ok", span)?;
    match &*args[0].borrow() {
        Value::Object(m) => {
            let ok = match m.get("ok").map(|v| v.borrow().clone()) {
                Some(Value::Bool(b)) => b,
                Some(Value::Int(n)) => n != 0,
                _ => match m.get("status").map(|v| v.borrow().clone()) {
                    Some(Value::Int(s)) => (200..300).contains(&s),
                    _ => false,
                },
            };
            Ok(Value::Bool(ok).ref_cell())
        }
        Value::Error(_) => Ok(Value::Bool(false).ref_cell()),
        _ => Ok(Value::Bool(false).ref_cell()),
    }
}

fn nsearch_json(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nsearch_json", span)?;
    let body = match &*args[0].borrow() {
        Value::Object(m) => string_field(Some(m), "body").unwrap_or_default(),
        Value::String(s) => s.clone(),
        other => {
            return Err(type_err(
                span,
                format!("json() expects response object or string, got {}", other.type_name()),
            ))
        }
    };
    match niao_json_core::parse(&body) {
        Ok(v) => Ok(json_to_niao(&v).ref_cell()),
        Err(e) => Ok(search_err(span, format!("json parse: {e}"))),
    }
}

fn nsearch_raise_for_status(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nsearch_raise_for_status", span)?;
    match &*args[0].borrow() {
        Value::Object(m) => {
            let ok = match m.get("ok").map(|v| v.borrow().clone()) {
                Some(Value::Bool(b)) => b,
                _ => false,
            };
            if ok {
                Ok(args[0].clone())
            } else {
                let status = int_field(Some(m), "status").unwrap_or(0);
                let body = string_field(Some(m), "body").unwrap_or_default();
                Ok(search_err(
                    span,
                    format!("HTTP {status}: {}", body.chars().take(200).collect::<String>()),
                ))
            }
        }
        Value::Error(_) => Ok(args[0].clone()),
        other => Err(type_err(
            span,
            format!(
                "raise_for_status() expects response, got {}",
                other.type_name()
            ),
        )),
    }
}

fn nsearch_hits(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nsearch.hits({engine: "elasticsearch", body: "{\"hits\":{\"hits\":[{\"_source\":{\"a\":1}}]}}"}).len()
    // => 1
    arity_range(args, 1, 2, "nsearch_hits", span)?;
    let (engine, body) = match &*args[0].borrow() {
        Value::Object(m) => {
            let engine = if args.len() >= 2 {
                string_arg(args, 1, "nsearch_hits", span)?
            } else {
                string_field(Some(m), "engine").unwrap_or_else(|| "elasticsearch".into())
            };
            let body = string_field(Some(m), "body").unwrap_or_default();
            (engine, body)
        }
        Value::String(s) => {
            let engine = if args.len() >= 2 {
                string_arg(args, 1, "nsearch_hits", span)?
            } else {
                "elasticsearch".into()
            };
            (engine, s.clone())
        }
        other => {
            return Err(type_err(
                span,
                format!("hits() expects response/string, got {}", other.type_name()),
            ))
        }
    };
    match extract_hits(&engine, &body) {
        Ok(hits) => {
            let arr: Vec<ValueRef> = hits.iter().map(|h| json_to_niao(h).ref_cell()).collect();
            Ok(Value::Array(arr).ref_cell())
        }
        Err(e) => Ok(search_err(span, e)),
    }
}

fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
    vec![
        ("nsearch_elasticsearch", "elasticsearch", Rc::new(nsearch_elasticsearch)),
        ("nsearch_opensearch", "opensearch", Rc::new(nsearch_opensearch)),
        ("nsearch_meilisearch", "meilisearch", Rc::new(nsearch_meilisearch)),
        ("nsearch_typesense", "typesense", Rc::new(nsearch_typesense)),
        ("nsearch_close", "close", Rc::new(nsearch_close)),
        ("nsearch_info", "info", Rc::new(nsearch_info)),
        ("nsearch_engine", "engine", Rc::new(nsearch_engine)),
        ("nsearch_search", "search", Rc::new(nsearch_search)),
        ("nsearch_index", "index", Rc::new(nsearch_index)),
        ("nsearch_get", "get", Rc::new(nsearch_get)),
        ("nsearch_delete", "delete", Rc::new(nsearch_delete)),
        ("nsearch_update", "update", Rc::new(nsearch_update)),
        ("nsearch_bulk", "bulk", Rc::new(nsearch_bulk)),
        ("nsearch_create_index", "create_index", Rc::new(nsearch_create_index)),
        ("nsearch_delete_index", "delete_index", Rc::new(nsearch_delete_index)),
        ("nsearch_list_indexes", "list_indexes", Rc::new(nsearch_list_indexes)),
        ("nsearch_index_exists", "index_exists", Rc::new(nsearch_index_exists)),
        ("nsearch_request", "request", Rc::new(nsearch_request)),
        ("nsearch_join", "join", Rc::new(nsearch_join)),
        ("nsearch_encode_params", "encode_params", Rc::new(nsearch_encode_params)),
        ("nsearch_es_query", "es_query", Rc::new(nsearch_es_query)),
        ("nsearch_es_bulk_ndjson", "es_bulk_ndjson", Rc::new(nsearch_es_bulk_ndjson)),
        ("nsearch_meili_filter", "meili_filter", Rc::new(nsearch_meili_filter)),
        ("nsearch_ts_filter", "ts_filter", Rc::new(nsearch_ts_filter)),
        ("nsearch_ok", "ok", Rc::new(nsearch_ok)),
        ("nsearch_json", "json", Rc::new(nsearch_json)),
        ("nsearch_raise_for_status", "raise_for_status", Rc::new(nsearch_raise_for_status)),
        ("nsearch_hits", "hits", Rc::new(nsearch_hits)),
    ]
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nsearch";
pub const MODULE_PATHS: &[&str] = &["nsearch", "std/nsearch"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_doctest() {
        let args = vec![
            Value::String("http://localhost:9200/".into()).ref_cell(),
            Value::String("_search".into()).ref_cell(),
        ];
        let v = nsearch_join(&args, Span::dummy()).unwrap();
        match &*v.borrow() {
            Value::String(s) => assert!(s.contains("9200") && s.contains("_search")),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn meili_filter_doctest() {
        let args = vec![Value::Array(vec![
            Value::String("a = 1".into()).ref_cell(),
            Value::String("b > 2".into()).ref_cell(),
        ])
        .ref_cell()];
        let v = nsearch_meili_filter(&args, Span::dummy()).unwrap();
        match &*v.borrow() {
            Value::String(s) => assert_eq!(s, "a = 1 AND b > 2"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn client_lifecycle() {
        let mut opts = HashMap::new();
        opts.insert(
            "url".into(),
            Value::String("http://localhost:9200".into()).ref_cell(),
        );
        let id = nsearch_elasticsearch(&[Value::Object(opts).ref_cell()], Span::dummy()).unwrap();
        let handle = match &*id.borrow() {
            Value::Int(n) => *n,
            _ => panic!("expected handle"),
        };
        let eng = nsearch_engine(&[Value::Int(handle).ref_cell()], Span::dummy()).unwrap();
        match &*eng.borrow() {
            Value::String(s) => assert_eq!(s, "elasticsearch"),
            other => panic!("expected engine, got {other:?}"),
        }
        let closed = nsearch_close(&[Value::Int(handle).ref_cell()], Span::dummy()).unwrap();
        match &*closed.borrow() {
            Value::Bool(true) => {}
            other => panic!("expected true, got {other:?}"),
        }
    }
}
