//! Native ndiff standard library — deep structural equality and diff for
//! Niao values (scalars, arrays, objects, packed int/float arrays).
//!
//! Import with `import "ndiff"` (or `import "std/ndiff"`).

use crate::{error_value, values_equal, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E3060_NDIFF_ARITY: u32 = 3060;
const E3061_NDIFF_ERROR: u32 = 3061;
const E3062_NDIFF_TYPE: u32 = 3062;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3060_NDIFF_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3062_NDIFF_TYPE, msg.into())
}

fn snapshot(v: &Value) -> ValueRef {
    v.clone().ref_cell()
}

fn float_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < f64::EPSILON
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Deep equality
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Diff
// ---------------------------------------------------------------------------

struct Change {
    path: String,
    left: ValueRef,
    right: ValueRef,
}

fn change(path: String, left: &Value, right: &Value) -> Change {
    Change {
        path,
        left: snapshot(left),
        right: snapshot(right),
    }
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
                    (Some(x), None) => out.push(change(p, &x.borrow(), &Value::Nil)),
                    (None, Some(y)) => out.push(change(p, &Value::Nil, &y.borrow())),
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
                    (Some(x), None) => out.push(change(p, &x.borrow(), &Value::Nil)),
                    (None, Some(y)) => out.push(change(p, &Value::Nil, &y.borrow())),
                    (None, None) => {}
                }
            }
        }
        (Value::IntArray(xs), Value::IntArray(ys)) => {
            let n = xs.len().max(ys.len());
            for i in 0..n {
                let p = path_index(path, i);
                match (xs.get(i), ys.get(i)) {
                    (Some(x), Some(y)) if x == y => {}
                    (Some(x), Some(y)) => {
                        out.push(change(p, &Value::Int(*x), &Value::Int(*y)));
                    }
                    (Some(x), None) => out.push(change(p, &Value::Int(*x), &Value::Nil)),
                    (None, Some(y)) => out.push(change(p, &Value::Nil, &Value::Int(*y))),
                    (None, None) => {}
                }
            }
        }
        (Value::FloatArray(xs), Value::FloatArray(ys)) => {
            let n = xs.len().max(ys.len());
            for i in 0..n {
                let p = path_index(path, i);
                match (xs.get(i), ys.get(i)) {
                    (Some(x), Some(y)) if float_eq(*x, *y) => {}
                    (Some(x), Some(y)) => {
                        out.push(change(p, &Value::Float(*x), &Value::Float(*y)));
                    }
                    (Some(x), None) => out.push(change(p, &Value::Float(*x), &Value::Nil)),
                    (None, Some(y)) => out.push(change(p, &Value::Nil, &Value::Float(*y))),
                    (None, None) => {}
                }
            }
        }
        _ => out.push(change(path.to_string(), left, right)),
    }
}

fn changes_to_value(changes: Vec<Change>) -> Value {
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
    Value::Array(items)
}

fn diff_result(equal: bool, changes: Vec<Change>) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("equal".to_string(), Value::Bool(equal).ref_cell());
    map.insert("changes".to_string(), changes_to_value(changes).ref_cell());
    Value::Object(map).ref_cell()
}

fn format_summary(diff_obj: &Value) -> Result<String, String> {
    let Value::Object(map) = diff_obj else {
        return Err(format!(
            "summary() expects a diff object, got {}",
            diff_obj.type_name()
        ));
    };
    let equal = match map.get("equal").map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        _ => {
            return Err("summary() expects diff object with boolean `equal` field".into());
        }
    };
    let changes = match map.get("changes").map(|v| v.borrow().clone()) {
        Some(Value::Array(items)) => items,
        _ => {
            return Err("summary() expects diff object with array `changes` field".into());
        }
    };

    if equal && changes.is_empty() {
        return Ok("equal".to_string());
    }

    let mut lines = Vec::new();
    lines.push(format!("equal: {equal}"));
    for ch in &changes {
        let Value::Object(cm) = &*ch.borrow() else {
            continue;
        };
        let path = match cm.get("path").map(|v| v.borrow().clone()) {
            Some(Value::String(s)) => {
                if s.is_empty() {
                    "(root)".to_string()
                } else {
                    s
                }
            }
            _ => "?".to_string(),
        };
        let left = cm
            .get("left")
            .map(|v| v.borrow().to_string())
            .unwrap_or_else(|| "?".into());
        let right = cm
            .get("right")
            .map(|v| v.borrow().to_string())
            .unwrap_or_else(|| "?".into());
        lines.push(format!("  {path}: {left} → {right}"));
    }
    Ok(lines.join("\n"))
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ndiff_equal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "equal", span)?;
    let eq = deep_equal(&args[0].borrow(), &args[1].borrow());
    Ok(Value::Bool(eq).ref_cell())
}

fn ndiff_diff(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "diff", span)?;
    let left = args[0].borrow();
    let right = args[1].borrow();
    let mut changes = Vec::new();
    collect_diff("", &left, &right, &mut changes);
    let equal = changes.is_empty();
    Ok(diff_result(equal, changes))
}

fn ndiff_summary(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "summary", span)?;
    match format_summary(&args[0].borrow()) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(msg) => {
            // Type-shaped mistakes are hard errors; malformed-but-object use catchable.
            if !matches!(&*args[0].borrow(), Value::Object(_)) {
                return Err(type_err(span, msg));
            }
            Ok(error_value(E3061_NDIFF_ERROR, "ndiff_error", msg, span))
        }
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ndiff_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ndiff_fns![
    ("ndiff_equal", "equal", ndiff_equal),
    ("ndiff_diff", "diff", ndiff_diff),
    ("ndiff_summary", "summary", ndiff_summary),
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

pub const MODULE_NAME: &str = "ndiff";
pub const MODULE_PATHS: &[&str] = &["ndiff", "std/ndiff"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    fn i(n: i64) -> ValueRef {
        Value::Int(n).ref_cell()
    }

    fn s(t: &str) -> ValueRef {
        Value::String(t.into()).ref_cell()
    }

    fn obj(pairs: &[(&str, ValueRef)]) -> ValueRef {
        let mut map = HashMap::new();
        for (k, v) in pairs {
            map.insert((*k).to_string(), v.clone());
        }
        Value::Object(map).ref_cell()
    }

    fn arr(items: Vec<ValueRef>) -> ValueRef {
        Value::Array(items).ref_cell()
    }

    #[test]
    fn equal_scalars_and_nil() {
        assert!(deep_equal(&Value::Int(1), &Value::Int(1)));
        assert!(!deep_equal(&Value::Int(1), &Value::Int(2)));
        assert!(deep_equal(&Value::Float(1.0), &Value::Float(1.0)));
        assert!(deep_equal(
            &Value::String("a".into()),
            &Value::String("a".into())
        ));
        assert!(deep_equal(&Value::Bool(true), &Value::Bool(true)));
        assert!(deep_equal(&Value::Nil, &Value::Nil));
        assert!(!deep_equal(&Value::Int(1), &Value::String("1".into())));
    }

    #[test]
    fn equal_nested_objects_and_arrays() {
        let a = obj(&[
            ("name", s("vivek")),
            ("tags", arr(vec![s("a"), s("b")])),
            ("meta", obj(&[("n", i(1))])),
        ]);
        let b = obj(&[
            ("name", s("vivek")),
            ("tags", arr(vec![s("a"), s("b")])),
            ("meta", obj(&[("n", i(1))])),
        ]);
        assert!(deep_equal(&a.borrow(), &b.borrow()));

        let c = obj(&[
            ("name", s("vivek")),
            ("tags", arr(vec![s("a"), s("c")])),
            ("meta", obj(&[("n", i(1))])),
        ]);
        assert!(!deep_equal(&a.borrow(), &c.borrow()));
    }

    #[test]
    fn equal_packed_arrays() {
        assert!(deep_equal(
            &Value::IntArray(vec![1, 2, 3]),
            &Value::IntArray(vec![1, 2, 3])
        ));
        assert!(!deep_equal(
            &Value::IntArray(vec![1, 2]),
            &Value::IntArray(vec![1, 2, 3])
        ));
        assert!(deep_equal(
            &Value::FloatArray(vec![1.0, 2.5]),
            &Value::FloatArray(vec![1.0, 2.5])
        ));
        assert!(!deep_equal(
            &Value::FloatArray(vec![1.0]),
            &Value::FloatArray(vec![1.1])
        ));
    }

    #[test]
    fn diff_reports_paths() {
        let left = obj(&[
            ("a", obj(&[("b", arr(vec![i(1), i(2)]))])),
            ("c", s("x")),
        ]);
        let right = obj(&[
            ("a", obj(&[("b", arr(vec![i(1), i(9)]))])),
            ("c", s("y")),
            ("d", i(3)),
        ]);
        let result = ndiff_diff(&[left, right], span()).unwrap();
        let borrowed = result.borrow();
        match &*borrowed {
            Value::Object(map) => {
                assert!(matches!(&*map.get("equal").unwrap().borrow(), Value::Bool(false)));
                match &*map.get("changes").unwrap().borrow() {
                    Value::Array(chs) => {
                        assert_eq!(chs.len(), 3);
                        let paths: Vec<String> = chs
                            .iter()
                            .map(|ch| match &*ch.borrow() {
                                Value::Object(cm) => match &*cm.get("path").unwrap().borrow() {
                                    Value::String(p) => p.clone(),
                                    _ => panic!("path"),
                                },
                                _ => panic!("change"),
                            })
                            .collect();
                        assert!(paths.contains(&"a.b[1]".to_string()));
                        assert!(paths.contains(&"c".to_string()));
                        assert!(paths.contains(&"d".to_string()));
                    }
                    other => panic!("expected changes array, got {other:?}"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn diff_packed_int_array_elements() {
        let left = Value::IntArray(vec![10, 20, 30]).ref_cell();
        let right = Value::IntArray(vec![10, 99]).ref_cell();
        let result = ndiff_diff(&[left, right], span()).unwrap();
        let borrowed = result.borrow();
        match &*borrowed {
            Value::Object(map) => match &*map.get("changes").unwrap().borrow() {
                Value::Array(chs) => {
                    assert_eq!(chs.len(), 2);
                    let paths: Vec<String> = chs
                        .iter()
                        .map(|ch| match &*ch.borrow() {
                            Value::Object(cm) => match &*cm.get("path").unwrap().borrow() {
                                Value::String(p) => p.clone(),
                                _ => panic!("path"),
                            },
                            _ => panic!("change"),
                        })
                        .collect();
                    assert_eq!(paths, vec!["[1]".to_string(), "[2]".to_string()]);
                }
                other => panic!("expected array, got {other:?}"),
            },
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn equal_builtin_and_identical_diff() {
        let a = obj(&[("x", i(1))]);
        let b = obj(&[("x", i(1))]);
        let eq = ndiff_equal(&[a.clone(), b.clone()], span()).unwrap();
        assert!(matches!(&*eq.borrow(), Value::Bool(true)));
        let d = ndiff_diff(&[a, b], span()).unwrap();
        let borrowed = d.borrow();
        match &*borrowed {
            Value::Object(map) => {
                assert!(matches!(&*map.get("equal").unwrap().borrow(), Value::Bool(true)));
                match &*map.get("changes").unwrap().borrow() {
                    Value::Array(chs) => assert!(chs.is_empty()),
                    other => panic!("expected array, got {other:?}"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn summary_formats_changes() {
        let left = obj(&[("n", i(1))]);
        let right = obj(&[("n", i(2))]);
        let d = ndiff_diff(&[left, right], span()).unwrap();
        let summary = ndiff_summary(&[d], span()).unwrap();
        let text = summary.borrow().to_string();
        assert!(text.contains("equal: false"));
        assert!(text.contains("n:"));
        assert!(text.contains('→'));
    }

    #[test]
    fn summary_equal_shortcut() {
        let d = diff_result(true, vec![]);
        let summary = ndiff_summary(&[d], span()).unwrap();
        assert_eq!(summary.borrow().to_string(), "equal");
    }

    #[test]
    fn summary_rejects_non_object() {
        let err = ndiff_summary(&[i(1)], span());
        assert!(err.is_err());
    }

    #[test]
    fn arity_errors() {
        assert!(ndiff_equal(&[i(1)], span()).is_err());
        assert!(ndiff_diff(&[], span()).is_err());
        assert!(ndiff_summary(&[], span()).is_err());
    }
}
