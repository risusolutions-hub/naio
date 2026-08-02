//! Native nwatch standard library — reactive poll watchers for file mtimes
//! and in-memory values. No background threads; callers poll explicitly.
//!
//! Import with `import "nwatch"` (or `import "std/nwatch"`).

use crate::{error_value, values_equal, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::rc::Rc;
use std::time::SystemTime;

// Wired in codes.rs by central integration.
const E3100_NWATCH_ARITY: u32 = 3100;
const E3101_NWATCH_ERROR: u32 = 3101;
const E3102_NWATCH_TYPE: u32 = 3102;
const E3103_NWATCH_INVALID_HANDLE: u32 = 3103;

// ---------------------------------------------------------------------------
// Watch model
// ---------------------------------------------------------------------------

enum Watch {
    File {
        path: String,
        last_mtime: Option<SystemTime>,
    },
    Value {
        value: ValueRef,
        dirty: bool,
    },
}

thread_local! {
    static WATCHES: RefCell<HashMap<i64, Watch>> = RefCell::new(HashMap::new());
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

fn read_mtime(path: &str) -> Option<SystemTime> {
    fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

fn mtimes_differ(a: Option<SystemTime>, b: Option<SystemTime>) -> bool {
    match (a, b) {
        (None, None) => false,
        (Some(x), Some(y)) => x != y,
        _ => true,
    }
}

fn with_watch<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Watch) -> Result<T, ValueRef>,
) -> NiaoResult<Result<T, ValueRef>> {
    WATCHES.with(|watches| {
        let mut watches = watches.borrow_mut();
        match watches.get_mut(&id) {
            Some(w) => Ok(f(w)),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3102_NWATCH_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3100_NWATCH_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
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
                "{name}() expects an int handle as argument {}, got {}",
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

fn watch_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3101_NWATCH_ERROR, "nwatch_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E3103_NWATCH_INVALID_HANDLE,
        "nwatch_error",
        format!("invalid or closed watch handle {id}"),
        span,
    )
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn str_val(s: impl Into<String>) -> NiaoResult<ValueRef> {
    Ok(Value::String(s.into()).ref_cell())
}

fn nil_val() -> NiaoResult<ValueRef> {
    Ok(Value::Nil.ref_cell())
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nwatch_file(path) — watch a filesystem path by mtime.
fn nwatch_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwatch_file", span)?;
    let path = string_arg(args, 0, "nwatch_file", span)?;
    let last_mtime = read_mtime(&path);
    let id = new_handle();
    WATCHES.with(|w| {
        w.borrow_mut().insert(id, Watch::File { path, last_mtime });
    });
    Ok(Value::Int(id).ref_cell())
}

/// nwatch_changed(h) — true when file mtime differs from last stored (no update).
fn nwatch_changed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwatch_changed", span)?;
    let id = int_arg(args, 0, "nwatch_changed", span)?;
    match with_watch(id, span, |w| match w {
        Watch::File { path, last_mtime } => {
            let current = read_mtime(path);
            Ok(mtimes_differ(*last_mtime, current))
        }
        Watch::Value { .. } => Err(watch_err(
            span,
            "nwatch_changed() expects a file watch handle",
        )),
    })? {
        Ok(b) => bool_val(b),
        Err(e) => Ok(e),
    }
}

/// nwatch_poll(h) — update stored mtime and return whether it changed.
fn nwatch_poll(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwatch_poll", span)?;
    let id = int_arg(args, 0, "nwatch_poll", span)?;
    match with_watch(id, span, |w| match w {
        Watch::File { path, last_mtime } => {
            let current = read_mtime(path);
            let changed = mtimes_differ(*last_mtime, current);
            *last_mtime = current;
            Ok(changed)
        }
        Watch::Value { .. } => Err(watch_err(span, "nwatch_poll() expects a file watch handle")),
    })? {
        Ok(b) => bool_val(b),
        Err(e) => Ok(e),
    }
}

/// nwatch_value(init) — in-memory value watch.
fn nwatch_value(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwatch_value", span)?;
    let value = Rc::clone(&args[0]);
    let id = new_handle();
    WATCHES.with(|w| {
        w.borrow_mut().insert(
            id,
            Watch::Value {
                value,
                dirty: false,
            },
        );
    });
    Ok(Value::Int(id).ref_cell())
}

/// nwatch_set(h, v) — update value watch; marks dirty when value changes.
fn nwatch_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nwatch_set", span)?;
    let id = int_arg(args, 0, "nwatch_set", span)?;
    let new_val = Rc::clone(&args[1]);
    match with_watch(id, span, |w| match w {
        Watch::Value { value, dirty } => {
            let same = values_equal(&value.borrow(), &new_val.borrow());
            *value = new_val;
            if !same {
                *dirty = true;
            }
            Ok(())
        }
        Watch::File { .. } => Err(watch_err(span, "nwatch_set() expects a value watch handle")),
    })? {
        Ok(()) => nil_val(),
        Err(e) => Ok(e),
    }
}

/// nwatch_take_changed(h) — value if dirty since last take, else nil.
fn nwatch_take_changed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwatch_take_changed", span)?;
    let id = int_arg(args, 0, "nwatch_take_changed", span)?;
    match with_watch(id, span, |w| match w {
        Watch::Value { value, dirty } => {
            if *dirty {
                *dirty = false;
                Ok(Some(Rc::clone(value)))
            } else {
                Ok(None)
            }
        }
        Watch::File { .. } => Err(watch_err(
            span,
            "nwatch_take_changed() expects a value watch handle",
        )),
    })? {
        Ok(Some(v)) => Ok(v),
        Ok(None) => nil_val(),
        Err(e) => Ok(e),
    }
}

/// nwatch_path(h) — watched path for file handles; nil for value watches.
fn nwatch_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwatch_path", span)?;
    let id = int_arg(args, 0, "nwatch_path", span)?;
    match with_watch(id, span, |w| match w {
        Watch::File { path, .. } => Ok(Some(path.clone())),
        Watch::Value { .. } => Ok(None),
    })? {
        Ok(Some(p)) => str_val(p),
        Ok(None) => nil_val(),
        Err(e) => Ok(e),
    }
}

/// nwatch_close(h) — free the watch; true if the handle existed.
fn nwatch_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwatch_close", span)?;
    let id = int_arg(args, 0, "nwatch_close", span)?;
    let removed = WATCHES.with(|w| w.borrow_mut().remove(&id).is_some());
    bool_val(removed)
}

/// nwatch_kind(h) — `"file"` or `"value"`.
fn nwatch_kind(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwatch_kind", span)?;
    let id = int_arg(args, 0, "nwatch_kind", span)?;
    match with_watch(id, span, |w| {
        Ok(match w {
            Watch::File { .. } => "file",
            Watch::Value { .. } => "value",
        })
    })? {
        Ok(k) => str_val(k),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nwatch_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nwatch_fns![
    ("nwatch_file", "file", nwatch_file),
    ("nwatch_changed", "changed", nwatch_changed),
    ("nwatch_poll", "poll", nwatch_poll),
    ("nwatch_value", "value", nwatch_value),
    ("nwatch_set", "set", nwatch_set),
    ("nwatch_take_changed", "take_changed", nwatch_take_changed),
    ("nwatch_path", "path", nwatch_path),
    ("nwatch_close", "close", nwatch_close),
    ("nwatch_kind", "kind", nwatch_kind),
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

pub const MODULE_NAME: &str = "nwatch";
pub const MODULE_PATHS: &[&str] = &["nwatch", "std/nwatch"];

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

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> ValueRef {
        let v = r.unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(_)), "expected handle int");
        v
    }

    #[test]
    fn value_watch_take_changed() {
        let h = handle(nwatch_value(&[i(1)], span()));
        assert!(matches!(
            &*nwatch_kind(&[h.clone()], span()).unwrap().borrow(),
            Value::String(k) if k == "value"
        ));
        assert!(matches!(
            &*nwatch_path(&[h.clone()], span()).unwrap().borrow(),
            Value::Nil
        ));

        // Fresh watch is clean.
        let t0 = nwatch_take_changed(&[h.clone()], span()).unwrap();
        assert!(matches!(&*t0.borrow(), Value::Nil));

        // Same value does not dirty.
        nwatch_set(&[h.clone(), i(1)], span()).unwrap();
        let t1 = nwatch_take_changed(&[h.clone()], span()).unwrap();
        assert!(matches!(&*t1.borrow(), Value::Nil));

        // Different value dirties; take returns it once.
        nwatch_set(&[h.clone(), i(2)], span()).unwrap();
        let t2 = nwatch_take_changed(&[h.clone()], span()).unwrap();
        assert!(matches!(&*t2.borrow(), Value::Int(2)));
        let t3 = nwatch_take_changed(&[h.clone()], span()).unwrap();
        assert!(matches!(&*t3.borrow(), Value::Nil));

        // String values work too.
        nwatch_set(&[h.clone(), s("hi")], span()).unwrap();
        let t4 = nwatch_take_changed(&[h.clone()], span()).unwrap();
        assert!(matches!(&*t4.borrow(), Value::String(s) if s == "hi"));

        assert!(matches!(
            &*nwatch_close(&[h.clone()], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
        let closed = nwatch_take_changed(&[h], span()).unwrap();
        assert!(
            matches!(&*closed.borrow(), Value::Error(e) if e.code == E3103_NWATCH_INVALID_HANDLE)
        );
    }

    #[test]
    fn value_ops_reject_file_kind_error() {
        // Use a non-existent path; file handle still allocates.
        let h = handle(nwatch_file(&[s("__nwatch_test_missing__.tmp")], span()));
        assert!(matches!(
            &*nwatch_kind(&[h.clone()], span()).unwrap().borrow(),
            Value::String(k) if k == "file"
        ));
        let err = nwatch_set(&[h.clone(), i(1)], span()).unwrap();
        assert!(matches!(&*err.borrow(), Value::Error(e) if e.code == E3101_NWATCH_ERROR));
        let err2 = nwatch_take_changed(&[h.clone()], span()).unwrap();
        assert!(matches!(&*err2.borrow(), Value::Error(e) if e.code == E3101_NWATCH_ERROR));
        nwatch_close(&[h], span()).unwrap();
    }
}
