//! Native ntest standard library — a tiny, fast test framework:
//! register cases with `ntest.case(name, fn)`, run them with `ntest.run()`,
//! and use rich assertions (`assert_eq`, `assert_near`, `assert_error`, ...).
//! Failures are reported per test; the runner returns a summary object.
//!
//! Import with `import "ntest"` (or `import "std/ntest"`).

use crate::{
    call_niao_function, error_value, values_equal, NativeFn, NiaoResult, RuntimeError, Value,
    ValueRef,
};
use niao_ast::Span;
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

struct TestCase {
    name: String,
    func: ValueRef,
    skip: bool,
}

thread_local! {
    static TESTS: RefCell<Vec<TestCase>> = const { RefCell::new(Vec::new()) };
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::TypeError {
        message: msg.into(),
        line: span.line,
        col: span.col,
    }
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E2660_NTEST_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E2660_NTEST_ARITY,
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
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn callable_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<ValueRef> {
    let ok = matches!(
        &*args[idx].borrow(),
        Value::Function(_) | Value::NativeFunction(_)
    );
    if !ok {
        return Err(type_err(
            span,
            format!(
                "{name}() expects a function as argument {}, got {}",
                idx + 1,
                args[idx].borrow().type_name()
            ),
        ));
    }
    Ok(Rc::clone(&args[idx]))
}

fn assert_fail(span: Span, msg: String) -> RuntimeError {
    RuntimeError::at(span, codes::E2662_NTEST_ASSERT, msg)
}

fn custom_msg(args: &[ValueRef], idx: usize) -> Option<String> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn nil() -> NiaoResult<ValueRef> {
    Ok(Value::Nil.ref_cell())
}

// ---------------------------------------------------------------------------
// Registration builtins
// ---------------------------------------------------------------------------

fn ntest_case(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ntest_case", span)?;
    let name = string_arg(args, 0, "ntest_case", span)?;
    let func = callable_arg(args, 1, "ntest_case", span)?;
    TESTS.with(|tests| {
        tests.borrow_mut().push(TestCase {
            name,
            func,
            skip: false,
        })
    });
    nil()
}

fn ntest_skip(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ntest_skip", span)?;
    let name = string_arg(args, 0, "ntest_skip", span)?;
    let func = callable_arg(args, 1, "ntest_skip", span)?;
    TESTS.with(|tests| {
        tests.borrow_mut().push(TestCase {
            name,
            func,
            skip: true,
        })
    });
    nil()
}

fn ntest_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ntest_clear", span)?;
    TESTS.with(|tests| tests.borrow_mut().clear());
    nil()
}

fn ntest_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ntest_count", span)?;
    let n = TESTS.with(|tests| tests.borrow().len());
    Ok(Value::Int(n as i64).ref_cell())
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

/// Run registered tests, optionally filtered: `ntest.run()` or `ntest.run("substring")`.
fn ntest_run(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "ntest_run", span)?;
    let filter = if args.is_empty() {
        None
    } else {
        Some(string_arg(args, 0, "ntest_run", span)?)
    };
    // Take a snapshot so tests that register more tests don't loop forever.
    let cases: Vec<(String, ValueRef, bool)> = TESTS.with(|tests| {
        tests
            .borrow()
            .iter()
            .map(|t| (t.name.clone(), Rc::clone(&t.func), t.skip))
            .collect()
    });
    let start = Instant::now();
    let mut passed: i64 = 0;
    let mut failed: i64 = 0;
    let mut skipped: i64 = 0;
    let mut failures: Vec<ValueRef> = Vec::new();
    for (name, func, skip) in cases {
        if let Some(f) = &filter {
            if !name.contains(f.as_str()) {
                continue;
            }
        }
        if skip {
            skipped += 1;
            println!("SKIP  {name}");
            continue;
        }
        let outcome = call_niao_function(func, &[], span);
        match outcome {
            Ok(result) => {
                let err_msg = match &*result.borrow() {
                    Value::Error(e) => Some(e.to_string()),
                    _ => None,
                };
                match err_msg {
                    None => {
                        passed += 1;
                        println!("PASS  {name}");
                    }
                    Some(msg) => {
                        failed += 1;
                        println!("FAIL  {name}: returned error: {msg}");
                        let mut f = HashMap::new();
                        f.insert("name".to_string(), Value::String(name.clone()).ref_cell());
                        f.insert("message".to_string(), Value::String(msg).ref_cell());
                        failures.push(Value::Object(f).ref_cell());
                    }
                }
            }
            Err(e) => {
                failed += 1;
                let msg = e.to_string();
                println!("FAIL  {name}: {msg}");
                let mut f = HashMap::new();
                f.insert("name".to_string(), Value::String(name.clone()).ref_cell());
                f.insert("message".to_string(), Value::String(msg).ref_cell());
                failures.push(Value::Object(f).ref_cell());
            }
        }
    }
    let duration_ms = start.elapsed().as_millis() as i64;
    let total = passed + failed + skipped;
    println!(
        "\n{} test(s): {passed} passed, {failed} failed, {skipped} skipped in {duration_ms}ms",
        total
    );
    let mut summary = HashMap::new();
    summary.insert("total".to_string(), Value::Int(total).ref_cell());
    summary.insert("passed".to_string(), Value::Int(passed).ref_cell());
    summary.insert("failed".to_string(), Value::Int(failed).ref_cell());
    summary.insert("skipped".to_string(), Value::Int(skipped).ref_cell());
    summary.insert("duration_ms".to_string(), Value::Int(duration_ms).ref_cell());
    summary.insert("ok".to_string(), Value::Bool(failed == 0).ref_cell());
    summary.insert("failures".to_string(), Value::Array(failures).ref_cell());
    Ok(Value::Object(summary).ref_cell())
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

fn ntest_assert_true(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntest_assert_true", span)?;
    let ok = matches!(&*args[0].borrow(), Value::Bool(true));
    if !ok {
        let msg = custom_msg(args, 1)
            .unwrap_or_else(|| format!("assert_true failed: got {}", args[0].borrow().to_string()));
        return Err(assert_fail(span, msg));
    }
    nil()
}

fn ntest_assert_false(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntest_assert_false", span)?;
    let ok = matches!(&*args[0].borrow(), Value::Bool(false));
    if !ok {
        let msg = custom_msg(args, 1)
            .unwrap_or_else(|| format!("assert_false failed: got {}", args[0].borrow().to_string()));
        return Err(assert_fail(span, msg));
    }
    nil()
}

fn ntest_assert_eq(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntest_assert_eq", span)?;
    let equal = values_equal(&args[0].borrow(), &args[1].borrow());
    if !equal {
        let msg = custom_msg(args, 2).unwrap_or_else(|| {
            format!(
                "assert_eq failed: {} != {}",
                args[0].borrow().to_string(),
                args[1].borrow().to_string()
            )
        });
        return Err(assert_fail(span, msg));
    }
    nil()
}

fn ntest_assert_ne(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntest_assert_ne", span)?;
    let equal = values_equal(&args[0].borrow(), &args[1].borrow());
    if equal {
        let msg = custom_msg(args, 2).unwrap_or_else(|| {
            format!(
                "assert_ne failed: both are {}",
                args[0].borrow().to_string()
            )
        });
        return Err(assert_fail(span, msg));
    }
    nil()
}

fn as_num(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

fn ntest_assert_near(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntest_assert_near", span)?;
    let a = as_num(&args[0].borrow());
    let b = as_num(&args[1].borrow());
    let eps = if args.len() > 2 {
        as_num(&args[2].borrow()).unwrap_or(1e-9)
    } else {
        1e-9
    };
    match (a, b) {
        (Some(a), Some(b)) => {
            if (a - b).abs() > eps {
                return Err(assert_fail(
                    span,
                    format!("assert_near failed: |{a} - {b}| > {eps}"),
                ));
            }
            nil()
        }
        _ => Err(type_err(span, "assert_near expects numbers")),
    }
}

fn ntest_assert_contains(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntest_assert_contains", span)?;
    let haystack = args[0].borrow();
    let needle = args[1].borrow();
    let found = match (&*haystack, &*needle) {
        (Value::String(h), Value::String(n)) => h.contains(n.as_str()),
        (Value::Array(items), n) => items.iter().any(|item| values_equal(&item.borrow(), n)),
        (Value::IntArray(items), Value::Int(n)) => items.contains(n),
        (Value::FloatArray(items), Value::Float(n)) => items.contains(n),
        (Value::Object(map), Value::String(key)) => map.contains_key(key),
        _ => {
            return Err(type_err(
                span,
                "assert_contains expects (string, string), (array, value), or (object, key)",
            ))
        }
    };
    if !found {
        let msg = custom_msg(args, 2).unwrap_or_else(|| {
            format!(
                "assert_contains failed: {} does not contain {}",
                haystack.to_string(),
                needle.to_string()
            )
        });
        return Err(assert_fail(span, msg));
    }
    nil()
}

fn ntest_assert_error(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntest_assert_error", span)?;
    let is_err = matches!(&*args[0].borrow(), Value::Error(_));
    if !is_err {
        let msg = custom_msg(args, 1).unwrap_or_else(|| {
            format!(
                "assert_error failed: got non-error {}",
                args[0].borrow().to_string()
            )
        });
        return Err(assert_fail(span, msg));
    }
    nil()
}

fn ntest_assert_not_error(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntest_assert_not_error", span)?;
    let err_msg = match &*args[0].borrow() {
        Value::Error(e) => Some(e.to_string()),
        _ => None,
    };
    if let Some(m) = err_msg {
        let msg = custom_msg(args, 1)
            .unwrap_or_else(|| format!("assert_not_error failed: got error: {m}"));
        return Err(assert_fail(span, msg));
    }
    nil()
}

fn ntest_fail(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "ntest_fail", span)?;
    let msg = custom_msg(args, 0).unwrap_or_else(|| "test failed".to_string());
    Err(assert_fail(span, msg))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ntest_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ntest_fns![
    ("ntest_case", "case", ntest_case),
    ("ntest_skip", "skip", ntest_skip),
    ("ntest_clear", "clear", ntest_clear),
    ("ntest_count", "count", ntest_count),
    ("ntest_run", "run", ntest_run),
    ("ntest_assert_true", "assert_true", ntest_assert_true),
    ("ntest_assert_false", "assert_false", ntest_assert_false),
    ("ntest_assert_eq", "assert_eq", ntest_assert_eq),
    ("ntest_assert_ne", "assert_ne", ntest_assert_ne),
    ("ntest_assert_near", "assert_near", ntest_assert_near),
    ("ntest_assert_contains", "assert_contains", ntest_assert_contains),
    ("ntest_assert_error", "assert_error", ntest_assert_error),
    ("ntest_assert_not_error", "assert_not_error", ntest_assert_not_error),
    ("ntest_fail", "fail", ntest_fail),
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

pub const MODULE_NAME: &str = "ntest";
pub const MODULE_PATHS: &[&str] = &["ntest", "std/ntest"];

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

    #[test]
    fn assert_eq_pass_and_fail() {
        assert!(ntest_assert_eq(&[i(1), i(1)], span()).is_ok());
        assert!(ntest_assert_eq(&[i(1), i(2)], span()).is_err());
    }

    #[test]
    fn assert_near_eps() {
        assert!(ntest_assert_near(&[Value::Float(1.0).ref_cell(), Value::Float(1.0 + 1e-12).ref_cell()], span()).is_ok());
        assert!(ntest_assert_near(&[Value::Float(1.0).ref_cell(), Value::Float(1.1).ref_cell()], span()).is_err());
    }

    #[test]
    fn assert_contains_variants() {
        assert!(ntest_assert_contains(&[s("hello world"), s("world")], span()).is_ok());
        let arr = Value::Array(vec![i(1), i(2)]).ref_cell();
        assert!(ntest_assert_contains(&[arr, i(2)], span()).is_ok());
        let ia = Value::IntArray(vec![5, 6]).ref_cell();
        assert!(ntest_assert_contains(&[ia, i(7)], span()).is_err());
    }

    #[test]
    fn registry_counts() {
        TESTS.with(|t| t.borrow_mut().clear());
        let f = Value::NativeFunction(Rc::new(|_args: &[ValueRef], _span: Span| {
            Ok(Value::Nil.ref_cell())
        }))
        .ref_cell();
        ntest_case(&[s("a"), f.clone()], span()).unwrap();
        ntest_skip(&[s("b"), f], span()).unwrap();
        match &*ntest_count(&[], span()).unwrap().borrow() {
            Value::Int(n) => assert_eq!(*n, 2),
            other => panic!("expected int, got {other:?}"),
        }
        ntest_clear(&[], span()).unwrap();
    }
}
