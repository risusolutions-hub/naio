//! Native ncrash standard library — structured JSON crash reports,
//! `wrap(fn)` guard, and stable fingerprints.
//!
//! Import with `import "ncrash"` (or `import "std/ncrash"`).

use crate::{
    call_niao_function, error_value, value_to_error, NativeFn, NiaoResult, RuntimeError, Value,
    ValueRef,
};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

// Wired in codes.rs by central integration.
const E3190_NCRASH_ARITY: u32 = 3190;
const E3191_NCRASH_ERROR: u32 = 3191;
const E3192_NCRASH_TYPE: u32 = 3192;

const FNV64_OFFSET: u64 = 0xcbf29ce484222325;
const FNV64_PRIME: u64 = 0x100000001b3;

thread_local! {
    static LAST_REPORT: RefCell<Option<HashMap<String, ValueRef>>> = const { RefCell::new(None) };
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3190_NCRASH_ARITY,
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
            E3190_NCRASH_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3192_NCRASH_TYPE, msg.into())
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

fn optional_object(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return Ok(HashMap::new());
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        Value::Nil => Ok(HashMap::new()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects object or nil as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

#[inline]
fn wall_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[inline]
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = FNV64_OFFSET;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV64_PRIME);
    }
    hash
}

fn clone_object(map: &HashMap<String, ValueRef>) -> HashMap<String, ValueRef> {
    map.iter().map(|(k, v)| (k.clone(), Rc::clone(v))).collect()
}

// ---------------------------------------------------------------------------
// Report builders
// ---------------------------------------------------------------------------

fn fingerprint_from_parts(kind: &str, code: u32, message: &str) -> String {
    let material = format!("{kind}|{code}|{message}");
    format!("{:08x}", fnv1a64(material.as_bytes()))
}

fn build_report_from_error(
    val: &Value,
    context: &HashMap<String, ValueRef>,
    span: Span,
) -> HashMap<String, ValueRef> {
    let (kind, code, message): (String, u32, String) = if let Some(err) = value_to_error(val) {
        ("error".into(), err.code, err.message.clone())
    } else {
        ("crash".into(), 0, val.to_string())
    };
    let fp = fingerprint_from_parts(&kind, code, &message);
    let mut map = HashMap::new();
    map.insert("fingerprint".to_string(), Value::String(fp).ref_cell());
    map.insert("kind".to_string(), Value::String(kind).ref_cell());
    map.insert("code".to_string(), Value::Int(code as i64).ref_cell());
    map.insert("message".to_string(), Value::String(message).ref_cell());
    map.insert("ts_ms".to_string(), Value::Int(wall_now_ms()).ref_cell());
    map.insert("line".to_string(), Value::Int(span.line as i64).ref_cell());
    map.insert("col".to_string(), Value::Int(span.col as i64).ref_cell());
    if !context.is_empty() {
        map.insert(
            "context".to_string(),
            Value::Object(clone_object(context)).ref_cell(),
        );
    }
    map
}

fn build_report_from_runtime(
    err: &RuntimeError,
    context: &HashMap<String, ValueRef>,
    span: Span,
) -> HashMap<String, ValueRef> {
    let code = err.code();
    let message = err.to_string();
    let fp = fingerprint_from_parts("runtime", code, &message);
    let mut map = HashMap::new();
    map.insert("fingerprint".to_string(), Value::String(fp).ref_cell());
    map.insert(
        "kind".to_string(),
        Value::String("runtime".into()).ref_cell(),
    );
    map.insert("code".to_string(), Value::Int(code as i64).ref_cell());
    map.insert("message".to_string(), Value::String(message).ref_cell());
    map.insert("ts_ms".to_string(), Value::Int(wall_now_ms()).ref_cell());
    map.insert("line".to_string(), Value::Int(span.line as i64).ref_cell());
    map.insert("col".to_string(), Value::Int(span.col as i64).ref_cell());
    if !context.is_empty() {
        map.insert(
            "context".to_string(),
            Value::Object(clone_object(context)).ref_cell(),
        );
    }
    map
}

fn store_last(report: &HashMap<String, ValueRef>) {
    LAST_REPORT.with(|r| *r.borrow_mut() = Some(clone_object(report)));
}

fn report_to_json(report: &HashMap<String, ValueRef>) -> String {
    serde_json::to_string(&value_to_json(&Value::Object(clone_object(report))))
        .unwrap_or_else(|_| "{}".into())
}

fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Nil => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(n) => serde_json::Value::Number((*n).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(|i| value_to_json(&i.borrow())).collect())
        }
        Value::Object(map) => {
            let mut obj = serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for k in keys {
                obj.insert(k.clone(), value_to_json(&map[k].borrow()));
            }
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::String(v.type_name().to_string()),
    }
}

fn fingerprint_from_report(report: &HashMap<String, ValueRef>) -> String {
    if let Some(v) = report.get("fingerprint") {
        if let Value::String(s) = &*v.borrow() {
            return s.clone();
        }
    }
    let kind = report
        .get("kind")
        .and_then(|v| match &*v.borrow() {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "unknown".into());
    let code = report
        .get("code")
        .and_then(|v| match &*v.borrow() {
            Value::Int(n) => Some(*n as u32),
            _ => None,
        })
        .unwrap_or(0);
    let message = report
        .get("message")
        .and_then(|v| match &*v.borrow() {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default();
    fingerprint_from_parts(&kind, code, &message)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ncrash_report(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncrash_report", span)?;
    let context = optional_object(args, 1, "ncrash_report", span)?;
    let report = build_report_from_error(&args[0].borrow(), &context, span);
    store_last(&report);
    Ok(Value::Object(report).ref_cell())
}

fn ncrash_wrap(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncrash_wrap", span)?;
    let func = callable_arg(args, 0, "ncrash_wrap", span)?;
    let context = optional_object(args, 1, "ncrash_wrap", span)?;
    match call_niao_function(func, &[], span) {
        Ok(result) => {
            let is_err = value_to_error(&result.borrow()).is_some();
            if is_err {
                let report = build_report_from_error(&result.borrow(), &context, span);
                store_last(&report);
                Ok(Value::Object(report).ref_cell())
            } else {
                Ok(result)
            }
        }
        Err(err) => {
            let report = build_report_from_runtime(&err, &context, span);
            store_last(&report);
            Ok(Value::Object(report).ref_cell())
        }
    }
}

fn ncrash_fingerprint(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrash_fingerprint", span)?;
    let fp = match &*args[0].borrow() {
        Value::Object(map) => fingerprint_from_report(map),
        other if value_to_error(other).is_some() => {
            let report = build_report_from_error(other, &HashMap::new(), span);
            fingerprint_from_report(&report)
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "ncrash_fingerprint() expects report object or error, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    Ok(Value::String(fp).ref_cell())
}

fn ncrash_format(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrash_format", span)?;
    match &*args[0].borrow() {
        Value::Object(map) => Ok(Value::String(report_to_json(map)).ref_cell()),
        other if value_to_error(other).is_some() => {
            let report = build_report_from_error(other, &HashMap::new(), span);
            Ok(Value::String(report_to_json(&report)).ref_cell())
        }
        other => Err(type_err(
            span,
            format!(
                "ncrash_format() expects report object or error, got {}",
                other.type_name()
            ),
        )),
    }
}

fn ncrash_last(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncrash_last", span)?;
    let last = LAST_REPORT.with(|r| r.borrow().clone());
    match last {
        Some(map) => Ok(Value::Object(map).ref_cell()),
        None => Ok(Value::Nil.ref_cell()),
    }
}

fn ncrash_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncrash_clear", span)?;
    LAST_REPORT.with(|r| *r.borrow_mut() = None);
    Ok(Value::Nil.ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncrash_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncrash_fns![
    ("ncrash_report", "report", ncrash_report),
    ("ncrash_wrap", "wrap", ncrash_wrap),
    ("ncrash_fingerprint", "fingerprint", ncrash_fingerprint),
    ("ncrash_format", "format", ncrash_format),
    ("ncrash_last", "last", ncrash_last),
    ("ncrash_clear", "clear", ncrash_clear),
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

pub const MODULE_NAME: &str = "ncrash";
pub const MODULE_PATHS: &[&str] = &["ncrash", "std/ncrash"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn report_from_error_value() {
        let _ = ncrash_clear(&[], span());
        let err = error_value(4242, "test_error", "boom", span());
        let report = ncrash_report(&[err], span()).unwrap();
        match &*report.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map["kind"].borrow(), Value::String(s) if s == "error"));
                assert!(matches!(&*map["code"].borrow(), Value::Int(4242)));
                assert!(matches!(&*map["message"].borrow(), Value::String(s) if s == "boom"));
                assert!(matches!(&*map["fingerprint"].borrow(), Value::String(s) if s.len() == 8));
            }
            other => panic!("expected object, got {other:?}"),
        }
        assert!(matches!(
            &*ncrash_last(&[], span()).unwrap().borrow(),
            Value::Object(_)
        ));
    }

    #[test]
    fn fingerprint_stable() {
        let err = error_value(100, "k", "same msg", span());
        let fp1 = match &*ncrash_fingerprint(&[err.clone()], span()).unwrap().borrow() {
            Value::String(s) => s.clone(),
            other => panic!("expected string, got {other:?}"),
        };
        let fp2 = match &*ncrash_fingerprint(&[err], span()).unwrap().borrow() {
            Value::String(s) => s.clone(),
            other => panic!("expected string, got {other:?}"),
        };
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn format_json() {
        let err = error_value(1, "e", "x", span());
        let report = ncrash_report(&[err], span()).unwrap();
        let json = match &*ncrash_format(&[report], span()).unwrap().borrow() {
            Value::String(s) => s.clone(),
            other => panic!("expected string, got {other:?}"),
        };
        assert!(json.contains("\"fingerprint\""));
        assert!(json.contains("\"message\""));
    }

    #[test]
    fn clear_last() {
        let err = error_value(1, "e", "x", span());
        let _ = ncrash_report(&[err], span()).unwrap();
        ncrash_clear(&[], span()).unwrap();
        assert!(matches!(
            &*ncrash_last(&[], span()).unwrap().borrow(),
            Value::Nil
        ));
    }

    #[test]
    fn arity_and_type_errors() {
        assert!(ncrash_report(&[], span()).is_err());
        assert!(ncrash_fingerprint(&[Value::Int(1).ref_cell()], span()).is_err());
    }
}
