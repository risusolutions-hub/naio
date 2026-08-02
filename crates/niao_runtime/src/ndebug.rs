//! Native ndebug standard library — checkpoint time-travel over values with deep
//! structural diff. Opcode-level scrubbing is a roadmap item (see docs/NDEBUG.md).
//!
//! Import with `import "ndebug"` (or `import "std/ndebug"`).

use crate::{error_value, values_equal, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3280_NDEBUG_ARITY: u32 = 3280;
const E3281_NDEBUG_ERROR: u32 = 3281;
const E3282_NDEBUG_TYPE: u32 = 3282;
const E3283_NDEBUG_INVALID_HANDLE: u32 = 3283;

// ---------------------------------------------------------------------------
// Deep clone / diff (ndiff-style, self-contained)
// ---------------------------------------------------------------------------

fn deep_clone(v: &Value) -> Value {
    match v {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|x| deep_clone(&x.borrow()).ref_cell())
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, vr) in map {
                out.insert(k.clone(), deep_clone(&vr.borrow()).ref_cell());
            }
            Value::Object(out)
        }
        Value::IntArray(xs) => Value::IntArray(xs.clone()),
        Value::FloatArray(xs) => Value::FloatArray(xs.clone()),
        Value::BoolArray(xs) => Value::BoolArray(xs.clone()),
        Value::ByteArray(xs) => Value::ByteArray(xs.clone()),
        Value::StringArray(sa) => Value::StringArray(sa.clone()),
        Value::Function(f) => Value::String(format!("<fn {}>", f.def.name)),
        Value::NativeFunction(_) => Value::String("<native_fn>".into()),
        Value::Native(ds) => Value::String(format!("<{}>", ds.borrow().kind_name())),
        Value::Instance(inst) => Value::String(format!("<{} instance>", inst.class_name)),
        Value::Error(e) => Value::String(format!("<error {}>", e.message)),
        Value::NclHandle(id) => Value::String(format!("<ncl_handle {id}>")),
        Value::NmlHandle(id) => Value::String(format!("<nml_handle {id}>")),
        #[cfg(feature = "nmongo")]
        Value::BsonDoc(_) => Value::String("<bson_doc>".into()),
        other => other.clone(),
    }
}

fn float_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < f64::EPSILON
}

fn deep_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Array(xs), Value::Array(ys)) => {
            xs.len() == ys.len()
                && xs
                    .iter()
                    .zip(ys.iter())
                    .all(|(x, y)| deep_equal(&x.borrow(), &y.borrow()))
        }
        (Value::Object(xm), Value::Object(ym)) => {
            xm.len() == ym.len()
                && xm.iter().all(|(k, xv)| {
                    ym.get(k)
                        .map(|yv| deep_equal(&xv.borrow(), &yv.borrow()))
                        .unwrap_or(false)
                })
        }
        (Value::IntArray(xs), Value::IntArray(ys)) => xs == ys,
        (Value::FloatArray(xs), Value::FloatArray(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys.iter()).all(|(x, y)| float_eq(*x, *y))
        }
        (Value::BoolArray(xs), Value::BoolArray(ys)) => xs == ys,
        (Value::ByteArray(xs), Value::ByteArray(ys)) => xs == ys,
        (Value::StringArray(xs), Value::StringArray(ys)) => xs == ys,
        _ => values_equal(a, b),
    }
}

struct Change {
    path: String,
    left: ValueRef,
    right: ValueRef,
}

fn path_key(base: &str, key: &str) -> String {
    if base.is_empty() {
        key.to_string()
    } else {
        format!("{base}.{key}")
    }
}

fn path_index(base: &str, idx: usize) -> String {
    format!("{base}[{idx}]")
}

fn collect_diff(path: &str, left: &Value, right: &Value, out: &mut Vec<Change>) {
    if deep_equal(left, right) {
        return;
    }
    match (left, right) {
        (Value::Array(xs), Value::Array(ys)) => {
            let n = xs.len().max(ys.len());
            for i in 0..n {
                let p = path_index(path, i);
                match (xs.get(i), ys.get(i)) {
                    (Some(x), Some(y)) => collect_diff(&p, &x.borrow(), &y.borrow(), out),
                    (Some(x), None) => out.push(Change {
                        path: p,
                        left: deep_clone(&x.borrow()).ref_cell(),
                        right: Value::Nil.ref_cell(),
                    }),
                    (None, Some(y)) => out.push(Change {
                        path: p,
                        left: Value::Nil.ref_cell(),
                        right: deep_clone(&y.borrow()).ref_cell(),
                    }),
                    (None, None) => {}
                }
            }
        }
        (Value::Object(xm), Value::Object(ym)) => {
            let mut keys: Vec<&String> = xm.keys().chain(ym.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let p = path_key(path, k);
                match (xm.get(k), ym.get(k)) {
                    (Some(x), Some(y)) => collect_diff(&p, &x.borrow(), &y.borrow(), out),
                    (Some(x), None) => out.push(Change {
                        path: p,
                        left: deep_clone(&x.borrow()).ref_cell(),
                        right: Value::Nil.ref_cell(),
                    }),
                    (None, Some(y)) => out.push(Change {
                        path: p,
                        left: Value::Nil.ref_cell(),
                        right: deep_clone(&y.borrow()).ref_cell(),
                    }),
                    (None, None) => {}
                }
            }
        }
        _ => out.push(Change {
            path: path.to_string(),
            left: deep_clone(left).ref_cell(),
            right: deep_clone(right).ref_cell(),
        }),
    }
}

fn diff_result(equal: bool, changes: Vec<Change>) -> ValueRef {
    let items: Vec<ValueRef> = changes
        .into_iter()
        .map(|c| {
            let mut map = HashMap::new();
            map.insert("path".to_string(), Value::String(c.path).ref_cell());
            map.insert("left".to_string(), c.left);
            map.insert("right".to_string(), c.right);
            Value::Object(map).ref_cell()
        })
        .collect();
    let mut map = HashMap::new();
    map.insert("equal".to_string(), Value::Bool(equal).ref_cell());
    map.insert("changes".to_string(), Value::Array(items).ref_cell());
    Value::Object(map).ref_cell()
}

// ---------------------------------------------------------------------------
// Session model
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Checkpoint {
    label: String,
    value: ValueRef,
}

struct DebugSession {
    checkpoints: Vec<Checkpoint>,
    counter: i64,
}

thread_local! {
    static SESSIONS: RefCell<HashMap<i64, DebugSession>> = RefCell::new(HashMap::new());
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

fn with_session<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut DebugSession) -> Result<T, ValueRef>,
) -> NiaoResult<Result<T, ValueRef>> {
    SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        match sessions.get_mut(&id) {
            Some(s) => Ok(f(s)),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

fn find_checkpoint<'a>(
    session: &'a DebugSession,
    label: &str,
    span: Span,
) -> Result<&'a Checkpoint, ValueRef> {
    session
        .checkpoints
        .iter()
        .rev()
        .find(|c| c.label == label)
        .ok_or_else(|| {
            error_value(
                E3281_NDEBUG_ERROR,
                "ndebug_error",
                format!("unknown checkpoint '{label}'"),
                span,
            )
        })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3282_NDEBUG_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3280_NDEBUG_ARITY,
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
            E3280_NDEBUG_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
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

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E3283_NDEBUG_INVALID_HANDLE,
        "ndebug_error",
        format!("invalid or closed ndebug handle {id}"),
        span,
    )
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ndebug_start(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ndebug_start", span)?;
    let id = new_handle();
    SESSIONS.with(|s| {
        s.borrow_mut().insert(
            id,
            DebugSession {
                checkpoints: Vec::new(),
                counter: 0,
            },
        );
    });
    Ok(Value::Int(id).ref_cell())
}

fn ndebug_checkpoint(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ndebug_checkpoint", span)?;
    let id = int_arg(args, 0, "ndebug_checkpoint", span)?;
    let (label, value) = if args.len() == 2 {
        (None, Rc::clone(&args[1]))
    } else {
        (
            Some(string_arg(args, 1, "ndebug_checkpoint", span)?),
            Rc::clone(&args[2]),
        )
    };
    match with_session(id, span, |session| {
        let label = match label {
            Some(l) => {
                if l.is_empty() {
                    return Err(error_value(
                        E3281_NDEBUG_ERROR,
                        "ndebug_error",
                        "checkpoint label must be non-empty",
                        span,
                    ));
                }
                l
            }
            None => {
                session.counter += 1;
                format!("cp_{}", session.counter)
            }
        };
        let snapshot = deep_clone(&value.borrow()).ref_cell();
        session.checkpoints.push(Checkpoint {
            label: label.clone(),
            value: snapshot,
        });
        Ok(label)
    })? {
        Ok(label) => Ok(Value::String(label).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ndebug_labels(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndebug_labels", span)?;
    let id = int_arg(args, 0, "ndebug_labels", span)?;
    match with_session(id, span, |session| {
        let items: Vec<ValueRef> = session
            .checkpoints
            .iter()
            .map(|c| Value::String(c.label.clone()).ref_cell())
            .collect();
        Ok(Value::Array(items))
    })? {
        Ok(v) => Ok(v.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ndebug_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndebug_len", span)?;
    let id = int_arg(args, 0, "ndebug_len", span)?;
    match with_session(id, span, |session| Ok(session.checkpoints.len() as i64))? {
        Ok(n) => Ok(Value::Int(n).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ndebug_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndebug_get", span)?;
    let id = int_arg(args, 0, "ndebug_get", span)?;
    let label = string_arg(args, 1, "ndebug_get", span)?;
    match with_session(id, span, |session| {
        let cp = find_checkpoint(session, &label, span)?;
        Ok(Rc::clone(&cp.value))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn ndebug_latest(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndebug_latest", span)?;
    let id = int_arg(args, 0, "ndebug_latest", span)?;
    match with_session(id, span, |session| {
        session
            .checkpoints
            .last()
            .map(|c| Rc::clone(&c.value))
            .ok_or_else(|| {
                error_value(
                    E3281_NDEBUG_ERROR,
                    "ndebug_error",
                    "no checkpoints recorded",
                    span,
                )
            })
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn ndebug_at(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndebug_at", span)?;
    let id = int_arg(args, 0, "ndebug_at", span)?;
    let idx = int_arg(args, 1, "ndebug_at", span)?;
    match with_session(id, span, |session| {
        if idx < 0 || idx as usize >= session.checkpoints.len() {
            return Err(error_value(
                E3281_NDEBUG_ERROR,
                "ndebug_error",
                format!("checkpoint index {idx} out of range"),
                span,
            ));
        }
        Ok(Rc::clone(&session.checkpoints[idx as usize].value))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn ndebug_diff(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ndebug_diff", span)?;
    let id = int_arg(args, 0, "ndebug_diff", span)?;
    let a = string_arg(args, 1, "ndebug_diff", span)?;
    let b = string_arg(args, 2, "ndebug_diff", span)?;
    match with_session(id, span, |session| {
        let left = find_checkpoint(session, &a, span)?;
        let right = find_checkpoint(session, &b, span)?;
        let mut changes = Vec::new();
        collect_diff(
            "",
            &left.value.borrow(),
            &right.value.borrow(),
            &mut changes,
        );
        Ok(diff_result(changes.is_empty(), changes))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn ndebug_diff_value(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ndebug_diff_value", span)?;
    let id = int_arg(args, 0, "ndebug_diff_value", span)?;
    let label = string_arg(args, 1, "ndebug_diff_value", span)?;
    let current = Rc::clone(&args[2]);
    match with_session(id, span, |session| {
        let cp = find_checkpoint(session, &label, span)?;
        let mut changes = Vec::new();
        collect_diff("", &cp.value.borrow(), &current.borrow(), &mut changes);
        Ok(diff_result(changes.is_empty(), changes))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn ndebug_travel(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndebug_travel", span)?;
    let id = int_arg(args, 0, "ndebug_travel", span)?;
    let label = string_arg(args, 1, "ndebug_travel", span)?;
    match with_session(id, span, |session| {
        let cp = find_checkpoint(session, &label, span)?;
        Ok(deep_clone(&cp.value.borrow()).ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn ndebug_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndebug_clear", span)?;
    let id = int_arg(args, 0, "ndebug_clear", span)?;
    let label = if args.len() == 2 {
        Some(string_arg(args, 1, "ndebug_clear", span)?)
    } else {
        None
    };
    match with_session(id, span, |session| {
        if let Some(label) = label {
            let before = session.checkpoints.len();
            session.checkpoints.retain(|c| c.label != label);
            if session.checkpoints.len() == before {
                return Err(error_value(
                    E3281_NDEBUG_ERROR,
                    "ndebug_error",
                    format!("unknown checkpoint '{label}'"),
                    span,
                ));
            }
        } else {
            session.checkpoints.clear();
        }
        Ok(true)
    })? {
        Ok(_) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ndebug_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndebug_close", span)?;
    let id = int_arg(args, 0, "ndebug_close", span)?;
    let removed = SESSIONS.with(|s| s.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ndebug_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ndebug_fns![
    ("ndebug_start", "start", ndebug_start),
    ("ndebug_checkpoint", "checkpoint", ndebug_checkpoint),
    ("ndebug_labels", "labels", ndebug_labels),
    ("ndebug_len", "len", ndebug_len),
    ("ndebug_get", "get", ndebug_get),
    ("ndebug_latest", "latest", ndebug_latest),
    ("ndebug_at", "at", ndebug_at),
    ("ndebug_diff", "diff", ndebug_diff),
    ("ndebug_diff_value", "diff_value", ndebug_diff_value),
    ("ndebug_travel", "travel", ndebug_travel),
    ("ndebug_clear", "clear", ndebug_clear),
    ("ndebug_close", "close", ndebug_close),
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

pub const MODULE_NAME: &str = "ndebug";
pub const MODULE_PATHS: &[&str] = &["ndebug", "std/ndebug"];

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

    fn obj(pairs: &[(&str, ValueRef)]) -> ValueRef {
        let mut map = HashMap::new();
        for (k, v) in pairs {
            map.insert((*k).to_string(), v.clone());
        }
        Value::Object(map).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> ValueRef {
        let v = r.unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(_)));
        v
    }

    #[test]
    fn checkpoint_labels_and_travel() {
        let h = handle(ndebug_start(&[], span()));
        let auto = ndebug_checkpoint(&[h.clone(), i(10)], span()).unwrap();
        assert!(matches!(&*auto.borrow(), Value::String(l) if l == "cp_1"));
        ndebug_checkpoint(&[h.clone(), s("start"), i(1)], span()).unwrap();
        ndebug_checkpoint(&[h.clone(), s("next"), i(2)], span()).unwrap();

        let labels = ndebug_labels(&[h.clone()], span()).unwrap();
        match &*labels.borrow() {
            Value::Array(items) => assert_eq!(items.len(), 3),
            other => panic!("expected array, got {other:?}"),
        }

        let v = ndebug_travel(&[h.clone(), s("start")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(1)));

        let latest = ndebug_latest(&[h.clone()], span()).unwrap();
        assert!(matches!(&*latest.borrow(), Value::Int(2)));

        let at0 = ndebug_at(&[h.clone(), i(0)], span()).unwrap();
        assert!(matches!(&*at0.borrow(), Value::Int(10)));

        ndebug_close(&[h], span()).unwrap();
    }

    #[test]
    fn diff_between_checkpoints() {
        let h = handle(ndebug_start(&[], span()));
        ndebug_checkpoint(&[h.clone(), s("a"), obj(&[("n", i(1))])], span()).unwrap();
        ndebug_checkpoint(&[h.clone(), s("b"), obj(&[("n", i(2))])], span()).unwrap();
        let d = ndebug_diff(&[h.clone(), s("a"), s("b")], span()).unwrap();
        match &*d.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map["equal"].borrow(), Value::Bool(false)));
                match &*map["changes"].borrow() {
                    Value::Array(chs) => assert_eq!(chs.len(), 1),
                    other => panic!("expected changes, got {other:?}"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
        ndebug_close(&[h], span()).unwrap();
    }

    #[test]
    fn diff_value_against_checkpoint() {
        let h = handle(ndebug_start(&[], span()));
        ndebug_checkpoint(&[h.clone(), s("base"), i(5)], span()).unwrap();
        let d = ndebug_diff_value(&[h.clone(), s("base"), i(7)], span()).unwrap();
        match &*d.borrow() {
            Value::Object(map) => assert!(matches!(&*map["equal"].borrow(), Value::Bool(false))),
            other => panic!("expected object, got {other:?}"),
        }
        ndebug_close(&[h], span()).unwrap();
    }

    #[test]
    fn clear_and_invalid_handle() {
        let h = handle(ndebug_start(&[], span()));
        ndebug_checkpoint(&[h.clone(), s("x"), i(1)], span()).unwrap();
        ndebug_clear(&[h.clone()], span()).unwrap();
        assert!(matches!(
            &*ndebug_len(&[h.clone()], span()).unwrap().borrow(),
            Value::Int(0)
        ));
        ndebug_close(&[h.clone()], span()).unwrap();
        let v = ndebug_len(&[h], span()).unwrap();
        assert!(matches!(
            &*v.borrow(),
            Value::Error(e) if e.code == E3283_NDEBUG_INVALID_HANDLE
        ));
    }

    #[test]
    fn namespace_has_expected_methods() {
        match namespace() {
            Value::Object(map) => {
                for key in [
                    "start",
                    "checkpoint",
                    "labels",
                    "len",
                    "get",
                    "latest",
                    "at",
                    "diff",
                    "diff_value",
                    "travel",
                    "clear",
                    "close",
                ] {
                    assert!(map.contains_key(key), "missing {key}");
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
        assert_eq!(builtins().len(), 12);
    }
}
