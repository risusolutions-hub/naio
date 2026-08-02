//! `nmodel` — Prisma-style ORM over nsqlite and npg connection handles.
//!
//! ```niao
//! import "nmodel"
//! import "nsqlite"
//!
//! fn main() {
//!     let db = nsqlite.open(":memory:")
//!     let s  = nmodel.schema({models: {User: {fields: {id: "int@id", name: "string@required"}}}})
//!     let c  = nmodel.bind(s, db)           // dialect defaults to "sqlite"
//!     nmodel.migrate(c)
//!     let row = nmodel.create(c, "User", {name: "Niao"})
//!     print(row.name)
//! }
//! ```

mod dialect;
mod migrate;
mod query;
pub(crate) mod schema;

use crate::{error_from_runtime, error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use dialect::{parse_dialect, Dialect};
use migrate::{pg_migrate, sqlite_migrate};
use niao_ast::Span;
use niao_errors::codes;
use query::{
    exec_create, exec_delete, exec_find_many, exec_find_unique, exec_raw, exec_update,
    extract_where, nmodel_schema_err, parse_find_many_opts, parse_update_opts,
};
use schema::{parse_schema, SchemaHandle};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// ── Handle registries ──────────────────────────────────────────────────────

#[derive(Clone)]
struct ClientHandle {
    schema_id: u64,
    db_handle: u64,
    dialect: Dialect,
}

thread_local! {
    static NEXT_SCHEMA: RefCell<u64> = const { RefCell::new(1) };
    static NEXT_CLIENT: RefCell<u64> = const { RefCell::new(1) };
    static SCHEMAS: RefCell<HashMap<u64, SchemaHandle>> = RefCell::new(HashMap::new());
    static CLIENTS: RefCell<HashMap<u64, ClientHandle>> = RefCell::new(HashMap::new());
}

fn alloc_schema(s: SchemaHandle) -> u64 {
    let id = NEXT_SCHEMA.with(|n| {
        let mut g = n.borrow_mut();
        let id = *g;
        *g = id + 1;
        id
    });
    SCHEMAS.with(|m| m.borrow_mut().insert(id, s));
    id
}

fn alloc_client(c: ClientHandle) -> u64 {
    let id = NEXT_CLIENT.with(|n| {
        let mut g = n.borrow_mut();
        let id = *g;
        *g = id + 1;
        id
    });
    CLIENTS.with(|m| m.borrow_mut().insert(id, c));
    id
}

fn with_schema<F, R>(id: u64, name: &str, span: Span, f: F) -> Result<R, RuntimeError>
where
    F: FnOnce(&SchemaHandle) -> Result<R, RuntimeError>,
{
    SCHEMAS.with(|m| {
        let g = m.borrow();
        let s = g.get(&id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E2831_NMODEL_ERROR,
                format!("{name}(): invalid or freed schema handle {id}"),
            )
        })?;
        f(s)
    })
}

fn with_client<F, R>(id: u64, name: &str, span: Span, f: F) -> Result<R, RuntimeError>
where
    F: FnOnce(&ClientHandle) -> Result<R, RuntimeError>,
{
    CLIENTS.with(|m| {
        let g = m.borrow();
        let c = g.get(&id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E2831_NMODEL_ERROR,
                format!("{name}(): invalid or freed client handle {id}"),
            )
        })?;
        f(c)
    })
}

// ── Argument helpers ───────────────────────────────────────────────────────

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> Result<(), RuntimeError> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E2830_NMODEL_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(
    args: &[ValueRef],
    min: usize,
    max: usize,
    name: &str,
    span: Span,
) -> Result<(), RuntimeError> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E2830_NMODEL_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> Result<u64, RuntimeError> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id as u64),
        other => Err(RuntimeError::at(
            span,
            codes::E2831_NMODEL_ERROR,
            format!(
                "{name}() expects handle id as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn string_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> Result<String, RuntimeError> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(RuntimeError::at(
            span,
            codes::E2832_NMODEL_TYPE,
            format!(
                "{name}() expects string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn object_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> Result<HashMap<String, ValueRef>, RuntimeError> {
    match &*args[idx].borrow() {
        Value::Object(m) => Ok(m.clone()),
        other => Err(RuntimeError::at(
            span,
            codes::E2832_NMODEL_TYPE,
            format!(
                "{name}() expects object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn ok_int(n: i64) -> ValueRef {
    Value::Int(n).ref_cell()
}

fn nmodel_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2831_NMODEL_ERROR, "nmodel_error", msg.into(), span)
}

// ── `nmodel.schema` ────────────────────────────────────────────────────────

fn nmodel_schema(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmodel_schema", span)?;
    let sh = parse_schema(&args[0], span).map_err(|e| e)?;
    let id = alloc_schema(sh);
    Ok(ok_int(id as i64))
}

// ── `nmodel.bind` ──────────────────────────────────────────────────────────

fn nmodel_bind(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmodel_bind", span)?;
    let schema_id = handle_arg(args, 0, "nmodel_bind", span)?;
    let db_handle = handle_arg(args, 1, "nmodel_bind", span)?;
    let dialect = if args.len() == 3 {
        let ds = string_arg(args, 2, "nmodel_bind", span)?;
        parse_dialect(&ds, span)?
    } else {
        Dialect::Sqlite
    };
    // Validate schema_id exists.
    with_schema(schema_id, "nmodel_bind", span, |_| Ok(()))?;
    let client_id = alloc_client(ClientHandle {
        schema_id,
        db_handle,
        dialect,
    });
    Ok(ok_int(client_id as i64))
}

// ── `nmodel.migrate` ───────────────────────────────────────────────────────

fn nmodel_migrate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmodel_migrate", span)?;
    let client_id = handle_arg(args, 0, "nmodel_migrate", span)?;
    let result = with_client(client_id, "nmodel_migrate", span, |client| {
        let schema_id = client.schema_id;
        let db_handle = client.db_handle;
        let dialect = client.dialect;
        with_schema(schema_id, "nmodel_migrate", span, |schema| match dialect {
            Dialect::Sqlite => {
                crate::nsqlite::handles::with_conn_mut(db_handle, "nmodel_migrate", span, |conn| {
                    sqlite_migrate(conn, schema, dialect)
                })
            }
            Dialect::Pg => {
                crate::npg::handles::with_conn_mut(db_handle, "nmodel_migrate", span, |conn| {
                    pg_migrate(conn, schema)
                })
            }
        })
    });
    result
        .map(|n| ok_int(n))
        .or_else(|e| Ok(error_from_runtime(&e)))
}

// ── `nmodel.create` ────────────────────────────────────────────────────────

fn nmodel_create(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nmodel_create", span)?;
    let client_id = handle_arg(args, 0, "nmodel_create", span)?;
    let model_name = string_arg(args, 1, "nmodel_create", span)?;
    let data = object_arg(args, 2, "nmodel_create", span)?;

    let result = with_client(client_id, "nmodel_create", span, |client| {
        let schema_id = client.schema_id;
        let db_handle = client.db_handle;
        let dialect = client.dialect;
        with_schema(schema_id, "nmodel_create", span, |schema| {
            let model = schema.models.get(&model_name).ok_or_else(|| {
                nmodel_schema_err(span, format!("unknown model \"{}\"", model_name))
            })?;
            exec_create(model, &data, db_handle, dialect, span)
        })
    });
    result
        .map(|v| v.ref_cell())
        .or_else(|e| Ok(error_from_runtime(&e)))
}

// ── `nmodel.find_many` ─────────────────────────────────────────────────────

fn nmodel_find_many(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmodel_find_many", span)?;
    let client_id = handle_arg(args, 0, "nmodel_find_many", span)?;
    let model_name = string_arg(args, 1, "nmodel_find_many", span)?;
    let opts_map = if args.len() == 3 {
        object_arg(args, 2, "nmodel_find_many", span)?
    } else {
        HashMap::new()
    };

    let result = with_client(client_id, "nmodel_find_many", span, |client| {
        let schema_id = client.schema_id;
        let db_handle = client.db_handle;
        let dialect = client.dialect;
        with_schema(schema_id, "nmodel_find_many", span, |schema| {
            let model = schema.models.get(&model_name).ok_or_else(|| {
                nmodel_schema_err(span, format!("unknown model \"{}\"", model_name))
            })?;
            let opts = parse_find_many_opts(&opts_map, span)?;
            exec_find_many(model, opts, db_handle, dialect, span)
        })
    });
    result
        .map(|v| v.ref_cell())
        .or_else(|e| Ok(error_from_runtime(&e)))
}

// ── `nmodel.find_unique` ───────────────────────────────────────────────────

fn nmodel_find_unique(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nmodel_find_unique", span)?;
    let client_id = handle_arg(args, 0, "nmodel_find_unique", span)?;
    let model_name = string_arg(args, 1, "nmodel_find_unique", span)?;
    let opts_map = object_arg(args, 2, "nmodel_find_unique", span)?;

    let result = with_client(client_id, "nmodel_find_unique", span, |client| {
        let schema_id = client.schema_id;
        let db_handle = client.db_handle;
        let dialect = client.dialect;
        with_schema(schema_id, "nmodel_find_unique", span, |schema| {
            let model = schema.models.get(&model_name).ok_or_else(|| {
                nmodel_schema_err(span, format!("unknown model \"{}\"", model_name))
            })?;
            let where_obj = extract_where(&opts_map, span)?;
            exec_find_unique(model, where_obj, db_handle, dialect, span)
        })
    });
    result
        .map(|v| v.ref_cell())
        .or_else(|e| Ok(error_from_runtime(&e)))
}

// ── `nmodel.update` ────────────────────────────────────────────────────────

fn nmodel_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nmodel_update", span)?;
    let client_id = handle_arg(args, 0, "nmodel_update", span)?;
    let model_name = string_arg(args, 1, "nmodel_update", span)?;
    let opts_map = object_arg(args, 2, "nmodel_update", span)?;

    let result = with_client(client_id, "nmodel_update", span, |client| {
        let schema_id = client.schema_id;
        let db_handle = client.db_handle;
        let dialect = client.dialect;
        with_schema(schema_id, "nmodel_update", span, |schema| {
            let model = schema.models.get(&model_name).ok_or_else(|| {
                nmodel_schema_err(span, format!("unknown model \"{}\"", model_name))
            })?;
            let (where_obj, data) = parse_update_opts(&opts_map, span)?;
            exec_update(model, where_obj, data, db_handle, dialect, span)
        })
    });
    result
        .map(|v| v.ref_cell())
        .or_else(|e| Ok(error_from_runtime(&e)))
}

// ── `nmodel.delete` ────────────────────────────────────────────────────────

fn nmodel_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nmodel_delete", span)?;
    let client_id = handle_arg(args, 0, "nmodel_delete", span)?;
    let model_name = string_arg(args, 1, "nmodel_delete", span)?;
    let opts_map = object_arg(args, 2, "nmodel_delete", span)?;

    let result = with_client(client_id, "nmodel_delete", span, |client| {
        let schema_id = client.schema_id;
        let db_handle = client.db_handle;
        let dialect = client.dialect;
        with_schema(schema_id, "nmodel_delete", span, |schema| {
            let model = schema.models.get(&model_name).ok_or_else(|| {
                nmodel_schema_err(span, format!("unknown model \"{}\"", model_name))
            })?;
            let where_obj = extract_where(&opts_map, span)?;
            exec_delete(model, where_obj, db_handle, dialect, span)
        })
    });
    result.map(ok_int).or_else(|e| Ok(error_from_runtime(&e)))
}

// ── `nmodel.raw` ───────────────────────────────────────────────────────────

fn nmodel_raw(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmodel_raw", span)?;
    let client_id = handle_arg(args, 0, "nmodel_raw", span)?;
    let sql = string_arg(args, 1, "nmodel_raw", span)?;
    let params: Vec<ValueRef> = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::Array(items) => items.clone(),
            other => {
                return Ok(nmodel_error(
                    span,
                    format!(
                        "nmodel_raw() expects params array as argument 3, got {}",
                        other.type_name()
                    ),
                ))
            }
        }
    } else {
        Vec::new()
    };

    let result = with_client(client_id, "nmodel_raw", span, |client| {
        let db_handle = client.db_handle;
        let dialect = client.dialect;
        exec_raw(&sql, &params, db_handle, dialect, span)
    });
    result
        .map(|v| v.ref_cell())
        .or_else(|e| Ok(error_from_runtime(&e)))
}

// ── `nmodel.schema_info` (introspection helper) ────────────────────────────

fn nmodel_schema_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmodel_schema_info", span)?;
    let schema_id = handle_arg(args, 0, "nmodel_schema_info", span)?;
    with_schema(schema_id, "nmodel_schema_info", span, |schema| {
        let mut out: HashMap<String, ValueRef> = HashMap::new();
        for (model_name, model_def) in &schema.models {
            let fields: Vec<ValueRef> = model_def
                .fields
                .iter()
                .map(|f| {
                    let mut m: HashMap<String, ValueRef> = HashMap::new();
                    m.insert("name".to_string(), Value::String(f.name.clone()).ref_cell());
                    m.insert(
                        "type".to_string(),
                        Value::String(format!("{:?}", f.ty).to_lowercase()).ref_cell(),
                    );
                    m.insert("is_id".to_string(), Value::Bool(f.is_id).ref_cell());
                    m.insert("is_unique".to_string(), Value::Bool(f.is_unique).ref_cell());
                    m.insert("nullable".to_string(), Value::Bool(f.nullable).ref_cell());
                    m.insert(
                        "default".to_string(),
                        f.default_sql
                            .as_ref()
                            .map(|d| Value::String(d.clone()).ref_cell())
                            .unwrap_or_else(|| Value::Nil.ref_cell()),
                    );
                    Value::Object(m).ref_cell()
                })
                .collect();
            out.insert(model_name.clone(), Value::Array(fields).ref_cell());
        }
        Ok(Value::Object(out))
    })
    .map(|v| v.ref_cell())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

// ── Builtins list ──────────────────────────────────────────────────────────

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    vec![
        ("nmodel_schema", Rc::new(nmodel_schema)),
        ("nmodel_bind", Rc::new(nmodel_bind)),
        ("nmodel_migrate", Rc::new(nmodel_migrate)),
        ("nmodel_create", Rc::new(nmodel_create)),
        ("nmodel_find_many", Rc::new(nmodel_find_many)),
        ("nmodel_find_unique", Rc::new(nmodel_find_unique)),
        ("nmodel_update", Rc::new(nmodel_update)),
        ("nmodel_delete", Rc::new(nmodel_delete)),
        ("nmodel_raw", Rc::new(nmodel_raw)),
        ("nmodel_schema_info", Rc::new(nmodel_schema_info)),
    ]
}

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

// ── Namespace ─────────────────────────────────────────────────────────────

pub fn namespace() -> Value {
    let mut map: HashMap<String, ValueRef> = HashMap::new();
    let bind = |map: &mut HashMap<String, ValueRef>, name: &str, f: NativeFn| {
        map.insert(name.to_string(), Value::NativeFunction(f).ref_cell());
    };
    bind(&mut map, "schema", Rc::new(nmodel_schema));
    bind(&mut map, "bind", Rc::new(nmodel_bind));
    bind(&mut map, "migrate", Rc::new(nmodel_migrate));
    bind(&mut map, "create", Rc::new(nmodel_create));
    bind(&mut map, "find_many", Rc::new(nmodel_find_many));
    bind(&mut map, "find_unique", Rc::new(nmodel_find_unique));
    bind(&mut map, "update", Rc::new(nmodel_update));
    bind(&mut map, "delete", Rc::new(nmodel_delete));
    bind(&mut map, "raw", Rc::new(nmodel_raw));
    bind(&mut map, "schema_info", Rc::new(nmodel_schema_info));
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nmodel";
pub const MODULE_PATHS: &[&str] = &["nmodel", "std/nmodel"];
