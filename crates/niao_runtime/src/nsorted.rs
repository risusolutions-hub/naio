//! Native nsorted standard library — sorted list / set / dict with bisect
//! insert, range queries, and nearest lookup (~sortedcontainers, bisect subset).
//!
//! Import with `import "nsorted"` (or `import "std/nsorted"`).

use crate::{error_value, values_equal, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_bignum::BigInt;
use niao_errors::codes;
use niao_sorted::{
    SortError, SortValue, SortedDict, SortedList, SortedSet,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3455_NSORTED_ARITY: u32 = codes::E3455_NSORTED_ARITY;
const E3456_NSORTED_ERROR: u32 = codes::E3456_NSORTED_ERROR;
const E3457_NSORTED_TYPE: u32 = codes::E3457_NSORTED_TYPE;
const E3458_NSORTED_INVALID_HANDLE: u32 = codes::E3458_NSORTED_INVALID_HANDLE;

// ---------------------------------------------------------------------------
// Container model
// ---------------------------------------------------------------------------

enum SortedKind {
    List(SortedList),
    Set(SortedSet),
    Dict(SortedDict),
}

impl SortedKind {
    fn kind_name(&self) -> &'static str {
        match self {
            SortedKind::List(_) => "list",
            SortedKind::Set(_) => "set",
            SortedKind::Dict(_) => "dict",
        }
    }
}

struct SortedStore {
    kind: SortedKind,
}

thread_local! {
    static STORES: RefCell<HashMap<i64, SortedStore>> = RefCell::new(HashMap::new());
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

fn register(kind: SortedKind) -> i64 {
    let id = new_handle();
    STORES.with(|stores| {
        stores.borrow_mut().insert(id, SortedStore { kind });
    });
    id
}

fn with_store_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut SortedStore) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    STORES.with(|stores| {
        let mut stores = stores.borrow_mut();
        match stores.get_mut(&id) {
            Some(s) => Ok(Ok(f(s))),
            None => Ok(Err(error_value(
                E3458_NSORTED_INVALID_HANDLE,
                "nsorted_error",
                format!("invalid or closed nsorted handle {id}"),
                span,
            ))),
        }
    })
}

fn with_store<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&SortedStore) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    STORES.with(|stores| {
        let stores = stores.borrow();
        match stores.get(&id) {
            Some(s) => Ok(Ok(f(s))),
            None => Ok(Err(error_value(
                E3458_NSORTED_INVALID_HANDLE,
                "nsorted_error",
                format!("invalid or closed nsorted handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Value conversion
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3457_NSORTED_TYPE, msg.into())
}

fn nsorted_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3456_NSORTED_ERROR, "nsorted_error", msg.into(), span)
}

fn sort_err(span: Span, e: SortError) -> ValueRef {
    let msg = match e {
        SortError::IncompatibleTypes => "incompatible value types for this container",
        SortError::NotFound => "value not found",
        SortError::IndexOutOfBounds => "index out of bounds",
        SortError::Empty => "container is empty",
    };
    nsorted_err(span, msg)
}

fn value_to_sort(v: &Value, span: Span) -> NiaoResult<SortValue> {
    match v {
        Value::Int(n) => Ok(SortValue::Int(*n)),
        Value::BigInt(n) => bigint_to_sort(n),
        Value::Float(f) => Ok(SortValue::Float(*f)),
        Value::String(s) => Ok(SortValue::Str(s.clone())),
        Value::Bool(b) => Ok(SortValue::Bool(*b)),
        other => Err(type_err(
            span,
            format!(
                "nsorted values must be bool, number, or string; got {}",
                other.type_name()
            ),
        )),
    }
}

fn bigint_to_sort(n: &BigInt) -> NiaoResult<SortValue> {
    if let Ok(s) = n.to_string().parse::<i64>() {
        return Ok(SortValue::Int(s));
    }
    if let Ok(f) = n.to_string().parse::<f64>() {
        if f.is_finite() {
            return Ok(SortValue::Float(f));
        }
    }
    Ok(SortValue::Str(n.to_string()))
}

fn sort_to_value(s: &SortValue) -> ValueRef {
    match s {
        SortValue::Bool(b) => Value::Bool(*b).ref_cell(),
        SortValue::Int(n) => Value::Int(*n).ref_cell(),
        SortValue::Float(f) => Value::Float(*f).ref_cell(),
        SortValue::Str(s) => Value::String(s.clone()).ref_cell(),
    }
}

fn sort_values_from_array(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<SortValue>> {
    match &*args[idx].borrow() {
        Value::IntArray(items) => Ok(items.iter().map(|n| SortValue::Int(*n)).collect()),
        Value::Array(items) => items
            .iter()
            .map(|v| value_to_sort(&v.borrow(), span))
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

fn pairs_from_object(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<(SortValue, SortValue)>> {
    match &*args[idx].borrow() {
        Value::Object(map) => {
            let mut pairs = Vec::with_capacity(map.len());
            for (k, v) in map {
                let key = value_to_sort(&Value::String(k.clone()), span)?;
                let val = value_to_sort(&v.borrow(), span)?;
                pairs.push((key, val));
            }
            Ok(pairs)
        }
        Value::Nil => Ok(Vec::new()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn sort_values_to_array(values: Vec<SortValue>) -> ValueRef {
    let items: Vec<ValueRef> = values.iter().map(sort_to_value).collect();
    Value::Array(items).ref_cell()
}

fn pairs_to_array(pairs: Vec<(SortValue, SortValue)>) -> ValueRef {
    let items: Vec<ValueRef> = pairs
        .into_iter()
        .map(|(k, v)| {
            let mut obj = HashMap::new();
            obj.insert("key".to_string(), sort_to_value(&k));
            obj.insert("value".to_string(), sort_to_value(&v));
            Value::Object(obj).ref_cell()
        })
        .collect();
    Value::Array(items).ref_cell()
}

fn parse_inclusive(opts: Option<&ValueRef>, span: Span) -> NiaoResult<(bool, bool)> {
    let Some(opts) = opts else {
        return Ok((true, true));
    };
    match &*opts.borrow() {
        Value::Object(map) => {
            let min_inc = map
                .get("min_inclusive")
                .map(|v| match &*v.borrow() {
                    Value::Bool(b) => Ok(*b),
                    other => Err(type_err(span, format!("min_inclusive must be bool, got {}", other.type_name()))),
                })
                .transpose()?
                .unwrap_or(true);
            let max_inc = map
                .get("max_inclusive")
                .map(|v| match &*v.borrow() {
                    Value::Bool(b) => Ok(*b),
                    other => Err(type_err(span, format!("max_inclusive must be bool, got {}", other.type_name()))),
                })
                .transpose()?
                .unwrap_or(true);
            Ok((min_inc, max_inc))
        }
        Value::Nil => Ok((true, true)),
        other => Err(type_err(
            span,
            format!("irange opts must be an object, got {}", other.type_name()),
        )),
    }
}

fn side_arg(args: &[ValueRef], idx: usize, default: &str, span: Span) -> NiaoResult<String> {
    if args.len() <= idx {
        return Ok(default.to_string());
    }
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        Value::Nil => Ok(default.to_string()),
        other => Err(type_err(
            span,
            format!("side must be string (left/right/nearest), got {}", other.type_name()),
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
            E3455_NSORTED_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3455_NSORTED_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
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

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    int_arg(args, idx, name, span)
}

fn value_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<SortValue> {
    value_to_sort(&args[idx].borrow(), span)
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

/// nsorted_new_list(items?) → handle
/// // >>> import "nsorted"
/// // >>> let h = nsorted_new_list([3, 1, 2])
/// // >>> nsorted_len(h)
/// // => 3
fn nsorted_new_list(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nsorted_new_list", span)?;
    let values = if args.is_empty() {
        Vec::new()
    } else {
        sort_values_from_array(args, 0, "nsorted_new_list", span)?
    };
    let list = SortedList::from_values(&values).map_err(|e| sort_err(span, e.into()))?;
    Ok(Value::Int(register(SortedKind::List(list))).ref_cell())
}

/// nsorted_new_set(items?) → handle
fn nsorted_new_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nsorted_new_set", span)?;
    let values = if args.is_empty() {
        Vec::new()
    } else {
        sort_values_from_array(args, 0, "nsorted_new_set", span)?
    };
    Ok(Value::Int(register(SortedKind::Set(SortedSet::from_values(&values)))).ref_cell())
}

/// nsorted_new_dict(obj?) → handle
fn nsorted_new_dict(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nsorted_new_dict", span)?;
    let pairs = if args.is_empty() {
        Vec::new()
    } else {
        pairs_from_object(args, 0, "nsorted_new_dict", span)?
    };
    Ok(Value::Int(register(SortedKind::Dict(SortedDict::from_pairs(&pairs)))).ref_cell())
}

/// nsorted_close(handle) → bool
fn nsorted_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsorted_close", span)?;
    let id = handle_arg(args, 0, "nsorted_close", span)?;
    let removed = STORES.with(|stores| stores.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

/// nsorted_kind(handle) → "list" | "set" | "dict"
fn nsorted_kind(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsorted_kind", span)?;
    let id = handle_arg(args, 0, "nsorted_kind", span)?;
    match with_store(id, span, |s| s.kind.kind_name().to_string())? {
        Ok(name) => Ok(Value::String(name).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nsorted_len(handle) → int
fn nsorted_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsorted_len", span)?;
    let id = handle_arg(args, 0, "nsorted_len", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::List(l) => l.len(),
        SortedKind::Set(s) => s.len(),
        SortedKind::Dict(d) => d.len(),
    })? {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// nsorted_add(handle, value) → bool (set: newly inserted; list: always true)
fn nsorted_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsorted_add", span)?;
    let id = handle_arg(args, 0, "nsorted_add", span)?;
    let value = value_arg(args, 1, "nsorted_add", span)?;
    match with_store_mut(id, span, |s| match &mut s.kind {
        SortedKind::List(l) => l.add(value.clone()).map(|_| true).map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => Ok(st.add(value)),
        SortedKind::Dict(_) => Err(nsorted_err(span, "nsorted_add() requires a list or set handle")),
    })? {
        Ok(Ok(b)) => Ok(Value::Bool(b).ref_cell()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_add_many(handle, items) → int count added
fn nsorted_add_many(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsorted_add_many", span)?;
    let id = handle_arg(args, 0, "nsorted_add_many", span)?;
    let values = sort_values_from_array(args, 1, "nsorted_add_many", span)?;
    let n = values.len();
    match with_store_mut(id, span, |s| match &mut s.kind {
        SortedKind::List(l) => l
            .add_many(&values)
            .map(|_| n as i64)
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => {
            st.add_many(&values);
            Ok(n as i64)
        }
        SortedKind::Dict(_) => Err(nsorted_err(span, "nsorted_add_many() requires a list or set handle")),
    })? {
        Ok(Ok(n)) => Ok(Value::Int(n).ref_cell()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_set(handle, key, value) → previous value or nil (dict only)
fn nsorted_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nsorted_set", span)?;
    let id = handle_arg(args, 0, "nsorted_set", span)?;
    let key = value_arg(args, 1, "nsorted_set", span)?;
    let value = value_arg(args, 2, "nsorted_set", span)?;
    match with_store_mut(id, span, |s| match &mut s.kind {
        SortedKind::Dict(d) => Ok(d.set(key, value).map(sort_to_value).unwrap_or(Value::Nil.ref_cell())),
        _ => Err(nsorted_err(span, "nsorted_set() requires a dict handle")),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_discard(handle, value) → bool
fn nsorted_discard(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsorted_discard", span)?;
    let id = handle_arg(args, 0, "nsorted_discard", span)?;
    let value = value_arg(args, 1, "nsorted_discard", span)?;
    match with_store_mut(id, span, |s| match &mut s.kind {
        SortedKind::List(l) => l
            .discard_one(&value)
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => Ok(st.discard(&value)),
        SortedKind::Dict(d) => Ok(d.remove(&value).is_ok()),
    })? {
        Ok(Ok(b)) => Ok(Value::Bool(b).ref_cell()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_remove(handle, value) → removed value
fn nsorted_remove(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsorted_remove", span)?;
    let id = handle_arg(args, 0, "nsorted_remove", span)?;
    let value = value_arg(args, 1, "nsorted_remove", span)?;
    match with_store_mut(id, span, |s| match &mut s.kind {
        SortedKind::List(l) => l
            .remove_one(&value)
            .map(sort_to_value)
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => st
            .remove(&value)
            .map(sort_to_value)
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Dict(d) => d
            .remove(&value)
            .map(sort_to_value)
            .map_err(|e| sort_err(span, e.into())),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_pop(handle, index?) → value
fn nsorted_pop(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsorted_pop", span)?;
    let id = handle_arg(args, 0, "nsorted_pop", span)?;
    let index = if args.len() > 1 {
        Some(int_arg(args, 1, "nsorted_pop", span)? as isize)
    } else {
        None
    };
    match with_store_mut(id, span, |s| match &mut s.kind {
        SortedKind::List(l) => l
            .pop(index)
            .map(sort_to_value)
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => st
            .pop(index)
            .map(sort_to_value)
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Dict(_) => Err(nsorted_err(span, "nsorted_pop() requires a list or set handle")),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_clear(handle) → nil
fn nsorted_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsorted_clear", span)?;
    let id = handle_arg(args, 0, "nsorted_clear", span)?;
    match with_store_mut(id, span, |s| {
        match &mut s.kind {
            SortedKind::List(l) => l.clear(),
            SortedKind::Set(st) => st.clear(),
            SortedKind::Dict(d) => d.clear(),
        }
        Ok(Value::Nil.ref_cell())
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Lookups
// ---------------------------------------------------------------------------

/// nsorted_get(handle, index_or_key) → value or nil
fn nsorted_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsorted_get", span)?;
    let id = handle_arg(args, 0, "nsorted_get", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::List(l) => {
            let idx = int_arg(args, 1, "nsorted_get", span)? as usize;
            l.get(idx)
                .map(sort_to_value)
                .map_err(|e| sort_err(span, e.into()))
        }
        SortedKind::Set(st) => {
            let idx = int_arg(args, 1, "nsorted_get", span)? as usize;
            st.get(idx)
                .map(sort_to_value)
                .map_err(|e| sort_err(span, e.into()))
        }
        SortedKind::Dict(d) => {
            let key = value_arg(args, 1, "nsorted_get", span)?;
            Ok(d.get(&key).map(sort_to_value).unwrap_or(Value::Nil.ref_cell()))
        }
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_contains(handle, value) → bool
fn nsorted_contains(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsorted_contains", span)?;
    let id = handle_arg(args, 0, "nsorted_contains", span)?;
    let value = value_arg(args, 1, "nsorted_contains", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::List(l) => Ok(l.count(&value).unwrap_or(0) > 0),
        SortedKind::Set(st) => Ok(st.contains(&value)),
        SortedKind::Dict(d) => Ok(d.contains_key(&value)),
    })? {
        Ok(Ok(b)) => Ok(Value::Bool(b).ref_cell()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_count(handle, value) → int (list only)
fn nsorted_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsorted_count", span)?;
    let id = handle_arg(args, 0, "nsorted_count", span)?;
    let value = value_arg(args, 1, "nsorted_count", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::List(l) => l
            .count(&value)
            .map(|n| Value::Int(n as i64).ref_cell())
            .map_err(|e| sort_err(span, e.into())),
        _ => Err(nsorted_err(span, "nsorted_count() requires a list handle")),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_index(handle, value) → int
fn nsorted_index(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsorted_index", span)?;
    let id = handle_arg(args, 0, "nsorted_index", span)?;
    let value = value_arg(args, 1, "nsorted_index", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::List(l) => l
            .index(&value)
            .map(|i| Value::Int(i as i64).ref_cell())
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => st
            .index(&value)
            .map(|i| Value::Int(i as i64).ref_cell())
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Dict(_) => Err(nsorted_err(span, "nsorted_index() requires a list or set handle")),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_min(handle) → value
fn nsorted_min(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsorted_min", span)?;
    let id = handle_arg(args, 0, "nsorted_min", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::List(l) => l
            .min()
            .map(sort_to_value)
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => st
            .min()
            .map(sort_to_value)
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Dict(d) => d
            .min_key()
            .map(sort_to_value)
            .map_err(|e| sort_err(span, e.into())),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_max(handle) → value
fn nsorted_max(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsorted_max", span)?;
    let id = handle_arg(args, 0, "nsorted_max", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::List(l) => l
            .max()
            .map(sort_to_value)
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => st
            .max()
            .map(sort_to_value)
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Dict(d) => d
            .max_key()
            .map(sort_to_value)
            .map_err(|e| sort_err(span, e.into())),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Bisect & range
// ---------------------------------------------------------------------------

/// nsorted_bisect_left(handle, value) → int
/// // >>> let h = nsorted_new_list([1, 3, 3, 5])
/// // >>> nsorted_bisect_left(h, 3)
/// // => 1
fn nsorted_bisect_left(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsorted_bisect_left", span)?;
    let id = handle_arg(args, 0, "nsorted_bisect_left", span)?;
    let value = value_arg(args, 1, "nsorted_bisect_left", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::List(l) => l
            .bisect_left(&value)
            .map(|i| Value::Int(i as i64).ref_cell())
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => Ok(Value::Int(st.bisect_left(&value) as i64).ref_cell()),
        SortedKind::Dict(d) => Ok(Value::Int(d.bisect_left(&value) as i64).ref_cell()),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_bisect_right(handle, value) → int
fn nsorted_bisect_right(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsorted_bisect_right", span)?;
    let id = handle_arg(args, 0, "nsorted_bisect_right", span)?;
    let value = value_arg(args, 1, "nsorted_bisect_right", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::List(l) => l
            .bisect_right(&value)
            .map(|i| Value::Int(i as i64).ref_cell())
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => Ok(Value::Int(st.bisect_right(&value) as i64).ref_cell()),
        SortedKind::Dict(d) => Ok(Value::Int(d.bisect_right(&value) as i64).ref_cell()),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_insort(handle, value, side?) → nil
fn nsorted_insort(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsorted_insort", span)?;
    let id = handle_arg(args, 0, "nsorted_insort", span)?;
    let value = value_arg(args, 1, "nsorted_insort", span)?;
    let side = side_arg(args, 2, "right", span)?;
    let right = side != "left";
    match with_store_mut(id, span, |s| match &mut s.kind {
        SortedKind::List(l) => l
            .insort(value, right)
            .map(|_| Value::Nil.ref_cell())
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => {
            st.add(value);
            Ok(Value::Nil.ref_cell())
        }
        SortedKind::Dict(_) => Err(nsorted_err(span, "nsorted_insort() requires a list or set handle")),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_irange(handle, min, max, opts?) → array
fn nsorted_irange(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nsorted_irange", span)?;
    let id = handle_arg(args, 0, "nsorted_irange", span)?;
    let min = value_arg(args, 1, "nsorted_irange", span)?;
    let max = value_arg(args, 2, "nsorted_irange", span)?;
    let (min_inc, max_inc) = parse_inclusive(args.get(3), span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::List(l) => l
            .irange(&min, &max, min_inc, max_inc)
            .map(sort_values_to_array)
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => Ok(sort_values_to_array(st.irange(&min, &max, min_inc, max_inc))),
        SortedKind::Dict(d) => Ok(pairs_to_array(d.irange(&min, &max, min_inc, max_inc))),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_islice(handle, start, stop?) → array
fn nsorted_islice(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsorted_islice", span)?;
    let id = handle_arg(args, 0, "nsorted_islice", span)?;
    let start = int_arg(args, 1, "nsorted_islice", span)? as isize;
    let stop = if args.len() > 2 {
        Some(int_arg(args, 2, "nsorted_islice", span)? as isize)
    } else {
        None
    };
    match with_store(id, span, |s| match &s.kind {
        SortedKind::List(l) => l
            .islice(start, stop)
            .map(sort_values_to_array)
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => Ok(sort_values_to_array(st.islice(start, stop))),
        SortedKind::Dict(_) => Err(nsorted_err(span, "nsorted_islice() requires a list or set handle")),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_nearest(handle, value, side?) → value or nil
fn nsorted_nearest(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsorted_nearest", span)?;
    let id = handle_arg(args, 0, "nsorted_nearest", span)?;
    let value = value_arg(args, 1, "nsorted_nearest", span)?;
    let side = side_arg(args, 2, "nearest", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::List(l) => l
            .nearest(&value, &side)
            .map(|opt| opt.map(sort_to_value).unwrap_or(Value::Nil.ref_cell()))
            .map_err(|e| sort_err(span, e.into())),
        SortedKind::Set(st) => Ok(st
            .nearest(&value, &side)
            .map(sort_to_value)
            .unwrap_or(Value::Nil.ref_cell())),
        SortedKind::Dict(_) => Err(nsorted_err(span, "nsorted_nearest() requires a list or set handle")),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Dict views
// ---------------------------------------------------------------------------

/// nsorted_keys(handle) → array
fn nsorted_keys(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsorted_keys", span)?;
    let id = handle_arg(args, 0, "nsorted_keys", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::Dict(d) => Ok(sort_values_to_array(d.keys())),
        SortedKind::List(l) => Ok(sort_values_to_array(l.to_vec())),
        SortedKind::Set(st) => Ok(sort_values_to_array(st.to_vec())),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_values(handle) → array
fn nsorted_values(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsorted_values", span)?;
    let id = handle_arg(args, 0, "nsorted_values", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::Dict(d) => Ok(sort_values_to_array(d.values())),
        _ => Err(nsorted_err(span, "nsorted_values() requires a dict handle")),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_items(handle) → array of {key, value}
fn nsorted_items(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsorted_items", span)?;
    let id = handle_arg(args, 0, "nsorted_items", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::Dict(d) => Ok(pairs_to_array(d.items())),
        _ => Err(nsorted_err(span, "nsorted_items() requires a dict handle")),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_peekitem(handle, index?) → {key, value}
fn nsorted_peekitem(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsorted_peekitem", span)?;
    let id = handle_arg(args, 0, "nsorted_peekitem", span)?;
    let index = if args.len() > 1 {
        int_arg(args, 1, "nsorted_peekitem", span)? as isize
    } else {
        0
    };
    match with_store(id, span, |s| match &s.kind {
        SortedKind::Dict(d) => d
            .peekitem(index)
            .map(|(k, v)| {
                let mut obj = HashMap::new();
                obj.insert("key".to_string(), sort_to_value(&k));
                obj.insert("value".to_string(), sort_to_value(&v));
                Value::Object(obj).ref_cell()
            })
            .map_err(|e| sort_err(span, e.into())),
        _ => Err(nsorted_err(span, "nsorted_peekitem() requires a dict handle")),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nsorted_to_array(handle) → array
fn nsorted_to_array(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsorted_to_array", span)?;
    let id = handle_arg(args, 0, "nsorted_to_array", span)?;
    match with_store(id, span, |s| match &s.kind {
        SortedKind::List(l) => Ok(sort_values_to_array(l.to_vec())),
        SortedKind::Set(st) => Ok(sort_values_to_array(st.to_vec())),
        SortedKind::Dict(d) => Ok(pairs_to_array(d.items())),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Standalone bisect on sorted arrays
// ---------------------------------------------------------------------------

fn sorted_int_array(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<i64>> {
    match &*args[idx].borrow() {
        Value::IntArray(v) => Ok(v.clone()),
        Value::Array(items) => items
            .iter()
            .map(|v| match &*v.borrow() {
                Value::Int(n) => Ok(*n),
                other => Err(type_err(
                    span,
                    format!("{name}() expects int array, got element {}", other.type_name()),
                )),
            })
            .collect(),
        other => Err(type_err(
            span,
            format!("{name}() expects int array as argument {}, got {}", idx + 1, other.type_name()),
        )),
    }
}

/// nsorted_bisect_left_arr(arr, value) → int
fn nsorted_bisect_left_arr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsorted_bisect_left_arr", span)?;
    let data = sorted_int_array(args, 0, "nsorted_bisect_left_arr", span)?;
    let x = int_arg(args, 1, "nsorted_bisect_left_arr", span)?;
    Ok(Value::Int(niao_sorted::bisect_left_int(&data, x) as i64).ref_cell())
}

/// nsorted_bisect_right_arr(arr, value) → int
fn nsorted_bisect_right_arr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsorted_bisect_right_arr", span)?;
    let data = sorted_int_array(args, 0, "nsorted_bisect_right_arr", span)?;
    let x = int_arg(args, 1, "nsorted_bisect_right_arr", span)?;
    Ok(Value::Int(niao_sorted::bisect_right_int(&data, x) as i64).ref_cell())
}

/// nsorted_insort_arr(arr, value, side?) → new sorted int array
fn nsorted_insort_arr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsorted_insort_arr", span)?;
    let mut data = sorted_int_array(args, 0, "nsorted_insort_arr", span)?;
    let x = int_arg(args, 1, "nsorted_insort_arr", span)?;
    let side = side_arg(args, 2, "right", span)?;
    let right = side != "left";
    niao_sorted::insort_int(&mut data, x, right);
    Ok(Value::IntArray(data).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nsorted_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nsorted_fns![
    ("nsorted_new_list", "new_list", nsorted_new_list),
    ("nsorted_new_set", "new_set", nsorted_new_set),
    ("nsorted_new_dict", "new_dict", nsorted_new_dict),
    ("nsorted_close", "close", nsorted_close),
    ("nsorted_kind", "kind", nsorted_kind),
    ("nsorted_len", "len", nsorted_len),
    ("nsorted_add", "add", nsorted_add),
    ("nsorted_add_many", "add_many", nsorted_add_many),
    ("nsorted_set", "set", nsorted_set),
    ("nsorted_discard", "discard", nsorted_discard),
    ("nsorted_remove", "remove", nsorted_remove),
    ("nsorted_pop", "pop", nsorted_pop),
    ("nsorted_clear", "clear", nsorted_clear),
    ("nsorted_get", "get", nsorted_get),
    ("nsorted_contains", "contains", nsorted_contains),
    ("nsorted_count", "count", nsorted_count),
    ("nsorted_index", "index", nsorted_index),
    ("nsorted_min", "min", nsorted_min),
    ("nsorted_max", "max", nsorted_max),
    ("nsorted_bisect_left", "bisect_left", nsorted_bisect_left),
    ("nsorted_bisect_right", "bisect_right", nsorted_bisect_right),
    ("nsorted_insort", "insort", nsorted_insort),
    ("nsorted_irange", "irange", nsorted_irange),
    ("nsorted_islice", "islice", nsorted_islice),
    ("nsorted_nearest", "nearest", nsorted_nearest),
    ("nsorted_keys", "keys", nsorted_keys),
    ("nsorted_values", "values", nsorted_values),
    ("nsorted_items", "items", nsorted_items),
    ("nsorted_peekitem", "peekitem", nsorted_peekitem),
    ("nsorted_to_array", "to_array", nsorted_to_array),
    ("nsorted_bisect_left_arr", "bisect_left_arr", nsorted_bisect_left_arr),
    ("nsorted_bisect_right_arr", "bisect_right_arr", nsorted_bisect_right_arr),
    ("nsorted_insort_arr", "insort_arr", nsorted_insort_arr),
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

pub const MODULE_NAME: &str = "nsorted";
pub const MODULE_PATHS: &[&str] = &["nsorted", "std/nsorted"];

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
    fn list_bisect_doctest() {
        let h = nsorted_new_list(
            &[Value::Array(
                vec![
                    Value::Int(1).ref_cell(),
                    Value::Int(3).ref_cell(),
                    Value::Int(3).ref_cell(),
                    Value::Int(5).ref_cell(),
                ],
            )
            .ref_cell()],
            span(),
        )
        .unwrap();
        let idx = nsorted_bisect_left(
            &[h, Value::Int(3).ref_cell()],
            span(),
        )
        .unwrap();
        assert_eq!(*idx.borrow(), Value::Int(1));
    }

    #[test]
    fn dict_peekitem() {
        let mut obj = HashMap::new();
        obj.insert("b".to_string(), Value::Int(2).ref_cell());
        obj.insert("a".to_string(), Value::Int(1).ref_cell());
        let h = nsorted_new_dict(&[Value::Object(obj).ref_cell()], span()).unwrap();
        let item = nsorted_peekitem(&[h], span()).unwrap();
        match &*item.borrow() {
            Value::Object(m) => {
                assert!(values_equal(&m.get("key").unwrap().borrow(), &Value::String("a".into())));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
