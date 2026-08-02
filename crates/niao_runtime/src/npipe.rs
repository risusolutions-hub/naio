//! Native npipe standard library — typed step pipelines over a built-in
//! op registry (`id`, `len`, `type`, `keys`, `not_nil`, `str`, `abs`).
//! Niao functions cannot be called from native, so stages are named ops only.
//!
//! Import with `import "npipe"` (or `import "std/npipe"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// Wired into niao_errors::codes by central integration.
const E3130_NPIPE_ARITY: u32 = 3130;
const E3131_NPIPE_ERROR: u32 = 3131;
const E3132_NPIPE_TYPE: u32 = 3132;
const E3133_NPIPE_INVALID_HANDLE: u32 = 3133;

// ---------------------------------------------------------------------------
// Pipeline model
// ---------------------------------------------------------------------------

struct Pipeline {
    steps: Vec<String>,
}

impl Pipeline {
    fn new() -> Self {
        Pipeline { steps: Vec::new() }
    }
}

thread_local! {
    static PIPES: RefCell<HashMap<i64, Pipeline>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_handle(pipe: Pipeline) -> i64 {
    let id = NEXT_HANDLE.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    PIPES.with(|pipes| {
        pipes.borrow_mut().insert(id, pipe);
    });
    id
}

fn with_pipe<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Pipeline) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    PIPES.with(|pipes| {
        let mut pipes = pipes.borrow_mut();
        match pipes.get_mut(&id) {
            Some(p) => Ok(Ok(f(p))),
            None => Ok(Err(error_value(
                E3133_NPIPE_INVALID_HANDLE,
                "npipe_error",
                format!("invalid or closed pipeline handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3130_NPIPE_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3132_NPIPE_TYPE, msg.into())
}

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a pipeline handle (int) as argument {}, got {}",
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
                                "{name}() expects string op names; item {} is {}",
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        Value::StringArray(sa) => Ok(sa.dense_vec()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an array of op name strings as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn pipe_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3131_NPIPE_ERROR, "npipe_error", msg.into(), span)
}

fn is_known_op(name: &str) -> bool {
    matches!(
        name,
        "id" | "len" | "type" | "keys" | "not_nil" | "str" | "abs"
    )
}

fn validate_op(name: &str, span: Span) -> NiaoResult<()> {
    if is_known_op(name) {
        Ok(())
    } else {
        Err(RuntimeError::at(
            span,
            E3131_NPIPE_ERROR,
            format!(
                "unknown npipe op '{name}'; expected id, len, type, keys, not_nil, str, or abs"
            ),
        ))
    }
}

// ---------------------------------------------------------------------------
// Built-in ops
// ---------------------------------------------------------------------------

fn value_len(v: &Value, span: Span) -> Result<ValueRef, ValueRef> {
    let len = match v {
        Value::String(s) => s.len() as i64,
        Value::IntArray(a) => a.len() as i64,
        Value::FloatArray(a) => a.len() as i64,
        Value::BoolArray(a) => a.len() as i64,
        Value::ByteArray(a) => a.len() as i64,
        Value::StringArray(a) => a.len() as i64,
        Value::Array(a) => a.len() as i64,
        Value::Object(m) => m.len() as i64,
        Value::Native(ds) => ds.borrow().len() as i64,
        other => {
            return Err(pipe_err(
                span,
                format!("len op not supported for {}", other.type_name()),
            ));
        }
    };
    Ok(Value::Int(len).ref_cell())
}

fn value_keys(v: &Value, span: Span) -> Result<ValueRef, ValueRef> {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<String> = map.keys().cloned().collect();
            keys.sort();
            let arr: Vec<ValueRef> = keys
                .into_iter()
                .map(|k| Value::String(k).ref_cell())
                .collect();
            Ok(Value::Array(arr).ref_cell())
        }
        other => Err(pipe_err(
            span,
            format!("keys op expects an object, got {}", other.type_name()),
        )),
    }
}

fn value_abs(v: &Value, span: Span) -> Result<ValueRef, ValueRef> {
    match v {
        Value::Int(n) => Ok(Value::Int(n.saturating_abs()).ref_cell()),
        Value::Float(x) => Ok(Value::Float(x.abs()).ref_cell()),
        Value::BigInt(n) => Ok(Value::BigInt(n.abs()).ref_cell()),
        other => Err(pipe_err(
            span,
            format!("abs op expects int or float, got {}", other.type_name()),
        )),
    }
}

fn apply_op(op: &str, input: ValueRef, span: Span) -> Result<ValueRef, ValueRef> {
    match op {
        "id" => Ok(input),
        "len" => value_len(&input.borrow(), span),
        "type" => Ok(Value::String(input.borrow().type_name()).ref_cell()),
        "keys" => value_keys(&input.borrow(), span),
        "not_nil" => Ok(Value::Bool(!matches!(&*input.borrow(), Value::Nil)).ref_cell()),
        "str" => Ok(Value::String(input.borrow().to_string()).ref_cell()),
        "abs" => value_abs(&input.borrow(), span),
        other => Err(pipe_err(span, format!("unknown npipe op '{other}'"))),
    }
}

fn run_steps(steps: &[String], input: ValueRef, span: Span) -> Result<ValueRef, ValueRef> {
    let mut cur = input;
    for op in steps {
        cur = apply_op(op, cur, span)?;
    }
    Ok(cur)
}

fn describe_steps(steps: &[String]) -> String {
    if steps.is_empty() {
        "npipe[]".to_string()
    } else {
        format!("npipe[{}]", steps.join(" → "))
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn npipe_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "npipe_new", span)?;
    let id = alloc_handle(Pipeline::new());
    Ok(Value::Int(id).ref_cell())
}

fn npipe_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "npipe_add", span)?;
    let id = handle_arg(args, 0, "npipe_add", span)?;
    let op = string_arg(args, 1, "npipe_add", span)?;
    validate_op(&op, span)?;
    match with_pipe(id, span, |p| {
        p.steps.push(op);
    })? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn npipe_run(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "npipe_run", span)?;
    let id = handle_arg(args, 0, "npipe_run", span)?;
    let input = Rc::clone(&args[1]);
    let steps = match with_pipe(id, span, |p| p.steps.clone())? {
        Ok(s) => s,
        Err(e) => return Ok(e),
    };
    match run_steps(&steps, input, span) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn npipe_steps(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npipe_steps", span)?;
    let id = handle_arg(args, 0, "npipe_steps", span)?;
    match with_pipe(id, span, |p| {
        p.steps
            .iter()
            .map(|s| Value::String(s.clone()).ref_cell())
            .collect::<Vec<_>>()
    })? {
        Ok(arr) => Ok(Value::Array(arr).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn npipe_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npipe_clear", span)?;
    let id = handle_arg(args, 0, "npipe_clear", span)?;
    match with_pipe(id, span, |p| {
        p.steps.clear();
    })? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn npipe_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npipe_close", span)?;
    let id = handle_arg(args, 0, "npipe_close", span)?;
    let removed = PIPES.with(|pipes| pipes.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

fn npipe_run_ops(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "npipe_run_ops", span)?;
    let ops = string_array_arg(args, 0, "npipe_run_ops", span)?;
    for op in &ops {
        validate_op(op, span)?;
    }
    let input = Rc::clone(&args[1]);
    match run_steps(&ops, input, span) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn npipe_describe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npipe_describe", span)?;
    let id = handle_arg(args, 0, "npipe_describe", span)?;
    match with_pipe(id, span, |p| describe_steps(&p.steps))? {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! npipe_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

npipe_fns![
    ("npipe_new", "new", npipe_new),
    ("npipe_add", "add", npipe_add),
    ("npipe_run", "run", npipe_run),
    ("npipe_steps", "steps", npipe_steps),
    ("npipe_clear", "clear", npipe_clear),
    ("npipe_close", "close", npipe_close),
    ("npipe_run_ops", "run_ops", npipe_run_ops),
    ("npipe_describe", "describe", npipe_describe),
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

pub const MODULE_NAME: &str = "npipe";
pub const MODULE_PATHS: &[&str] = &["npipe", "std/npipe"];

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

    fn f(v: f64) -> ValueRef {
        Value::Float(v).ref_cell()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn arr(items: Vec<ValueRef>) -> ValueRef {
        Value::Array(items).ref_cell()
    }

    fn obj(pairs: Vec<(&str, ValueRef)>) -> ValueRef {
        let mut map = HashMap::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), v);
        }
        Value::Object(map).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> ValueRef {
        let v = r.unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(_)), "expected handle int");
        v
    }

    fn expect_int(r: NiaoResult<ValueRef>) -> i64 {
        match &*r.unwrap().borrow() {
            Value::Int(n) => *n,
            other => panic!("expected int, got {other:?}"),
        }
    }

    fn expect_str(r: NiaoResult<ValueRef>) -> String {
        match &*r.unwrap().borrow() {
            Value::String(s) => s.clone(),
            other => panic!("expected string, got {other:?}"),
        }
    }

    fn expect_bool(r: NiaoResult<ValueRef>) -> bool {
        match &*r.unwrap().borrow() {
            Value::Bool(b) => *b,
            other => panic!("expected bool, got {other:?}"),
        }
    }

    #[test]
    fn new_add_run_pipeline() {
        let h = handle(npipe_new(&[], span()));
        npipe_add(&[h.clone(), s("abs")], span()).unwrap();
        npipe_add(&[h.clone(), s("str")], span()).unwrap();
        let out = expect_str(npipe_run(&[h.clone(), i(-42)], span()));
        assert_eq!(out, "42");
        let steps = npipe_steps(&[h.clone()], span()).unwrap();
        match &*steps.borrow() {
            Value::Array(a) => {
                assert_eq!(a.len(), 2);
                assert!(matches!(&*a[0].borrow(), Value::String(x) if x == "abs"));
                assert!(matches!(&*a[1].borrow(), Value::String(x) if x == "str"));
            }
            other => panic!("expected array, got {other:?}"),
        }
        assert_eq!(
            expect_str(npipe_describe(&[h.clone()], span())),
            "npipe[abs → str]"
        );
        npipe_close(&[h], span()).unwrap();
    }

    #[test]
    fn ops_id_len_type_keys_not_nil() {
        assert_eq!(expect_int(Ok(apply_op("id", i(7), span()).unwrap())), 7);
        assert_eq!(expect_int(Ok(apply_op("len", s("hi"), span()).unwrap())), 2);
        assert_eq!(
            expect_str(Ok(apply_op("type", i(1), span()).unwrap())),
            "int"
        );
        let keys = apply_op("keys", obj(vec![("b", i(2)), ("a", i(1))]), span()).unwrap();
        match &*keys.borrow() {
            Value::Array(a) => {
                assert_eq!(a.len(), 2);
                assert!(matches!(&*a[0].borrow(), Value::String(x) if x == "a"));
                assert!(matches!(&*a[1].borrow(), Value::String(x) if x == "b"));
            }
            other => panic!("expected array, got {other:?}"),
        }
        assert!(expect_bool(Ok(apply_op("not_nil", i(1), span()).unwrap())));
        assert!(!expect_bool(Ok(apply_op(
            "not_nil",
            Value::Nil.ref_cell(),
            span()
        )
        .unwrap())));
    }

    #[test]
    fn abs_int_and_float() {
        assert_eq!(expect_int(Ok(apply_op("abs", i(-9), span()).unwrap())), 9);
        match &*apply_op("abs", f(-3.5), span()).unwrap().borrow() {
            Value::Float(x) => assert!((*x - 3.5).abs() < 1e-12),
            other => panic!("expected float, got {other:?}"),
        }
    }

    #[test]
    fn run_ops_without_handle() {
        let ops = arr(vec![s("type"), s("len")]);
        // type("hello") → "string", len("string") → 6
        assert_eq!(expect_int(npipe_run_ops(&[ops, s("hello")], span())), 6);
    }

    #[test]
    fn clear_and_close() {
        let h = handle(npipe_new(&[], span()));
        npipe_add(&[h.clone(), s("id")], span()).unwrap();
        npipe_clear(&[h.clone()], span()).unwrap();
        assert_eq!(expect_str(npipe_describe(&[h.clone()], span())), "npipe[]");
        assert!(expect_bool(npipe_close(&[h.clone()], span())));
        let v = npipe_run(&[h, i(1)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn unknown_op_hard_error() {
        let h = handle(npipe_new(&[], span()));
        let err = npipe_add(&[h, s("nope")], span());
        assert!(err.is_err());
    }

    #[test]
    fn op_failure_error_value() {
        let v = apply_op("abs", s("x"), span());
        assert!(matches!(v, Err(_)));
        let err = v.unwrap_err();
        assert!(matches!(&*err.borrow(), Value::Error(_)));
    }

    #[test]
    fn invalid_handle_error_value() {
        let v = npipe_steps(&[i(999_999)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn arity_error() {
        assert!(npipe_new(&[i(1)], span()).is_err());
        assert!(npipe_run_ops(&[arr(vec![s("id")])], span()).is_err());
    }
}
