//! Native npersist standard library — im-rc persistent Vector and HashMap with
//! structural sharing. Mutating operations return new handles; prior handles
//! remain valid snapshots.
//!
//! Import with `import "npersist"` (or `import "std/npersist"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use im_rc::HashMap as ImMap;
use im_rc::Vector as ImVector;
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E3400_NPERSIST_ARITY: u32 = 3400;
const E3401_NPERSIST_ERROR: u32 = 3401;
const E3402_NPERSIST_TYPE: u32 = 3402;
const E3403_NPERSIST_INVALID_HANDLE: u32 = 3403;

// ---------------------------------------------------------------------------
// Persistent store model
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum PersistKind {
    Vec(ImVector<ValueRef>),
    Map(ImMap<String, ValueRef>),
}

impl PersistKind {
    fn kind_name(&self) -> &'static str {
        match self {
            PersistKind::Vec(_) => "vector",
            PersistKind::Map(_) => "map",
        }
    }
}

struct PersistStore {
    kind: PersistKind,
}

thread_local! {
    static STORES: RefCell<HashMap<i64, PersistStore>> = RefCell::new(HashMap::new());
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

fn register(kind: PersistKind) -> i64 {
    let id = new_handle();
    STORES.with(|stores| {
        stores.borrow_mut().insert(id, PersistStore { kind });
    });
    id
}

fn with_store<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&PersistStore) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    STORES.with(|stores| {
        let stores = stores.borrow();
        match stores.get(&id) {
            Some(s) => Ok(Ok(f(s))),
            None => Ok(Err(error_value(
                E3403_NPERSIST_INVALID_HANDLE,
                "npersist_error",
                format!("invalid or closed persist handle {id}"),
                span,
            ))),
        }
    })
}

fn expect_vec(store: &PersistStore, span: Span) -> Result<&ImVector<ValueRef>, ValueRef> {
    match &store.kind {
        PersistKind::Vec(v) => Ok(v),
        PersistKind::Map(_) => Err(npersist_err(
            span,
            "expected a persistent vector handle, got map",
        )),
    }
}

fn expect_map(store: &PersistStore, span: Span) -> Result<&ImMap<String, ValueRef>, ValueRef> {
    match &store.kind {
        PersistKind::Map(m) => Ok(m),
        PersistKind::Vec(_) => Err(npersist_err(
            span,
            "expected a persistent map handle, got vector",
        )),
    }
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3400_NPERSIST_ARITY,
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
) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3400_NPERSIST_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3402_NPERSIST_TYPE, msg.into())
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

fn npersist_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3401_NPERSIST_ERROR, "npersist_error", msg.into(), span)
}

fn array_to_vector(items: &[ValueRef]) -> ImVector<ValueRef> {
    items.iter().map(Rc::clone).collect()
}

fn object_to_map(obj: &HashMap<String, ValueRef>) -> ImMap<String, ValueRef> {
    obj.iter().map(|(k, v)| (k.clone(), Rc::clone(v))).collect()
}

// ---------------------------------------------------------------------------
// Vector builtins
// ---------------------------------------------------------------------------

/// npersist_vec_new(items?) → handle
fn npersist_vec_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "npersist_vec_new", span)?;
    let vec = if args.is_empty() {
        ImVector::new()
    } else {
        match &*args[0].borrow() {
            Value::Array(items) => array_to_vector(items),
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "npersist_vec_new() expects an array as argument 1, got {}",
                        other.type_name()
                    ),
                ))
            }
        }
    };
    Ok(Value::Int(register(PersistKind::Vec(vec))).ref_cell())
}

/// npersist_vec_push(handle, value) → new handle
fn npersist_vec_push(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "npersist_vec_push", span)?;
    let id = int_arg(args, 0, "npersist_vec_push", span)?;
    let item = Rc::clone(&args[1]);
    match with_store(id, span, |s| match expect_vec(s, span) {
        Ok(v) => {
            let mut next = v.clone();
            next.push_back(item);
            Ok(register(PersistKind::Vec(next)))
        }
        Err(e) => Err(e),
    })? {
        Ok(Ok(new_id)) => Ok(Value::Int(new_id).ref_cell()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// npersist_vec_set(handle, index, value) → new handle
fn npersist_vec_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "npersist_vec_set", span)?;
    let id = int_arg(args, 0, "npersist_vec_set", span)?;
    let index = int_arg(args, 1, "npersist_vec_set", span)?;
    let item = Rc::clone(&args[2]);
    if index < 0 {
        return Ok(npersist_err(span, "index must be >= 0"));
    }
    match with_store(id, span, |s| {
        let v = expect_vec(s, span)?;
        if index as usize >= v.len() {
            return Err(npersist_err(
                span,
                format!("index {index} out of range (len {})", v.len()),
            ));
        }
        let next = v.update(index as usize, item);
        Ok(register(PersistKind::Vec(next)))
    })? {
        Ok(Ok(new_id)) => Ok(Value::Int(new_id).ref_cell()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// npersist_vec_get(handle, index) → value
fn npersist_vec_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "npersist_vec_get", span)?;
    let id = int_arg(args, 0, "npersist_vec_get", span)?;
    let index = int_arg(args, 1, "npersist_vec_get", span)?;
    if index < 0 {
        return Ok(npersist_err(span, "index must be >= 0"));
    }
    match with_store(id, span, |s| {
        let v = expect_vec(s, span)?;
        v.get(index as usize).map(Rc::clone).ok_or_else(|| {
            npersist_err(
                span,
                format!("index {index} out of range (len {})", v.len()),
            )
        })
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// npersist_vec_len(handle) → int
fn npersist_vec_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npersist_vec_len", span)?;
    let id = int_arg(args, 0, "npersist_vec_len", span)?;
    match with_store(id, span, |s| expect_vec(s, span).map(|v| v.len()))? {
        Ok(Ok(n)) => Ok(Value::Int(n as i64).ref_cell()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Map builtins
// ---------------------------------------------------------------------------

/// npersist_map_new(obj?) → handle
fn npersist_map_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "npersist_map_new", span)?;
    let map = if args.is_empty() {
        ImMap::new()
    } else {
        match &*args[0].borrow() {
            Value::Object(obj) => object_to_map(obj),
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "npersist_map_new() expects an object as argument 1, got {}",
                        other.type_name()
                    ),
                ))
            }
        }
    };
    Ok(Value::Int(register(PersistKind::Map(map))).ref_cell())
}

/// npersist_map_set(handle, key, value) → new handle
fn npersist_map_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "npersist_map_set", span)?;
    let id = int_arg(args, 0, "npersist_map_set", span)?;
    let key = string_arg(args, 1, "npersist_map_set", span)?;
    let val = Rc::clone(&args[2]);
    match with_store(id, span, |s| {
        let m = expect_map(s, span)?;
        let next = m.update(key, val);
        Ok(register(PersistKind::Map(next)))
    })? {
        Ok(Ok(new_id)) => Ok(Value::Int(new_id).ref_cell()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// npersist_map_get(handle, key) → value or nil
fn npersist_map_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "npersist_map_get", span)?;
    let id = int_arg(args, 0, "npersist_map_get", span)?;
    let key = string_arg(args, 1, "npersist_map_get", span)?;
    match with_store(id, span, |s| {
        let m = expect_map(s, span)?;
        Ok(m.get(&key)
            .map(Rc::clone)
            .unwrap_or_else(|| Value::Nil.ref_cell()))
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// npersist_map_keys(handle) → string array
fn npersist_map_keys(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npersist_map_keys", span)?;
    let id = int_arg(args, 0, "npersist_map_keys", span)?;
    match with_store(id, span, |s| {
        let m = expect_map(s, span)?;
        let mut keys: Vec<String> = m.keys().cloned().collect();
        keys.sort();
        Ok(keys)
    })? {
        Ok(Ok(keys)) => {
            let items: Vec<ValueRef> = keys
                .into_iter()
                .map(|k| Value::String(k).ref_cell())
                .collect();
            Ok(Value::Array(items).ref_cell())
        }
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// npersist_map_len(handle) → int
fn npersist_map_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npersist_map_len", span)?;
    let id = int_arg(args, 0, "npersist_map_len", span)?;
    match with_store(id, span, |s| expect_map(s, span).map(|m| m.len()))? {
        Ok(Ok(n)) => Ok(Value::Int(n as i64).ref_cell()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Shared utilities
// ---------------------------------------------------------------------------

/// npersist_share(handle_a, handle_b) → bool (structural pointer equality)
fn npersist_share(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "npersist_share", span)?;
    let a = int_arg(args, 0, "npersist_share", span)?;
    let b = int_arg(args, 1, "npersist_share", span)?;
    let shared = STORES.with(|stores| {
        let stores = stores.borrow();
        match (stores.get(&a), stores.get(&b)) {
            (Some(sa), Some(sb)) => match (&sa.kind, &sb.kind) {
                (PersistKind::Vec(va), PersistKind::Vec(vb)) => va.ptr_eq(vb),
                (PersistKind::Map(ma), PersistKind::Map(mb)) => ma.ptr_eq(mb),
                _ => false,
            },
            _ => false,
        }
    });
    Ok(Value::Bool(shared).ref_cell())
}

/// npersist_kind(handle) → "vector" | "map"
fn npersist_kind(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npersist_kind", span)?;
    let id = int_arg(args, 0, "npersist_kind", span)?;
    match with_store(id, span, |s| s.kind.kind_name())? {
        Ok(name) => Ok(Value::String(name.to_string()).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// npersist_close(handle) → bool
fn npersist_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npersist_close", span)?;
    let id = int_arg(args, 0, "npersist_close", span)?;
    let removed = STORES.with(|stores| stores.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! npersist_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

npersist_fns![
    ("npersist_vec_new", "vec_new", npersist_vec_new),
    ("npersist_vec_push", "vec_push", npersist_vec_push),
    ("npersist_vec_set", "vec_set", npersist_vec_set),
    ("npersist_vec_get", "vec_get", npersist_vec_get),
    ("npersist_vec_len", "vec_len", npersist_vec_len),
    ("npersist_map_new", "map_new", npersist_map_new),
    ("npersist_map_set", "map_set", npersist_map_set),
    ("npersist_map_get", "map_get", npersist_map_get),
    ("npersist_map_keys", "map_keys", npersist_map_keys),
    ("npersist_map_len", "map_len", npersist_map_len),
    ("npersist_share", "share", npersist_share),
    ("npersist_kind", "kind", npersist_kind),
    ("npersist_close", "close", npersist_close),
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

pub const MODULE_NAME: &str = "npersist";
pub const MODULE_PATHS: &[&str] = &["npersist", "std/npersist"];

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

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> i64 {
        match &*r.unwrap().borrow() {
            Value::Int(n) => *n,
            other => panic!("expected handle, got {other:?}"),
        }
    }

    #[test]
    fn vector_push_preserves_old_snapshot() {
        let v0 = handle(npersist_vec_new(&[], span()));
        let v1 = handle(npersist_vec_push(&[i(v0), i(10)], span()));
        let v2 = handle(npersist_vec_push(&[i(v1), i(20)], span()));
        assert_eq!(
            match &*npersist_vec_len(&[i(v0)], span()).unwrap().borrow() {
                Value::Int(n) => *n,
                _ => panic!(),
            },
            0
        );
        assert_eq!(
            match &*npersist_vec_len(&[i(v2)], span()).unwrap().borrow() {
                Value::Int(n) => *n,
                _ => panic!(),
            },
            2
        );
        assert!(matches!(
            &*npersist_share(&[i(v0), i(v1)], span()).unwrap().borrow(),
            Value::Bool(false)
        ));
        assert!(matches!(
            &*npersist_share(&[i(v1), i(v1)], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
        npersist_close(&[i(v0)], span()).unwrap();
        npersist_close(&[i(v1)], span()).unwrap();
        npersist_close(&[i(v2)], span()).unwrap();
    }

    #[test]
    fn map_set_and_get() {
        let m0 = handle(npersist_map_new(&[], span()));
        let m1 = handle(npersist_map_set(
            &[i(m0), Value::String("x".into()).ref_cell(), i(1)],
            span(),
        ));
        let m2 = handle(npersist_map_set(
            &[i(m1), Value::String("y".into()).ref_cell(), i(2)],
            span(),
        ));
        assert!(matches!(
            &*npersist_map_get(&[i(m0), Value::String("x".into()).ref_cell()], span())
                .unwrap()
                .borrow(),
            Value::Nil
        ));
        assert!(matches!(
            &*npersist_map_get(&[i(m2), Value::String("y".into()).ref_cell()], span())
                .unwrap()
                .borrow(),
            Value::Int(2)
        ));
        npersist_close(&[i(m0)], span()).unwrap();
        npersist_close(&[i(m1)], span()).unwrap();
        npersist_close(&[i(m2)], span()).unwrap();
    }
}
