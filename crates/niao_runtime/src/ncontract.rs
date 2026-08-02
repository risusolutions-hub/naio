//! Native ncontract standard library — design-by-contract helpers:
//! `require` / `ensure` (throw), `check` (error value), `assert_type`,
//! and a small `invariant` rule subset (`required` / `type`).
//!
//! Import with `import "ncontract"` (or `import "std/ncontract"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E3082_NCONTRACT_TYPE, msg.into())
}

fn contract_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E3081_NCONTRACT_ERROR, msg.into())
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
            codes::E3080_NCONTRACT_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E3080_NCONTRACT_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn optional_msg(args: &[ValueRef], idx: usize) -> Option<String> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        other => Some(other.to_string()),
    }
}

fn cond_ok(v: &Value) -> bool {
    v.is_truthy()
}

fn bool_true() -> NiaoResult<ValueRef> {
    Ok(Value::Bool(true).ref_cell())
}

// ---------------------------------------------------------------------------
// Type matching (assert_type + invariant)
// ---------------------------------------------------------------------------

const KNOWN_TYPES: &[&str] = &[
    "int", "float", "string", "bool", "nil", "array", "object", "function",
];

fn type_matches(expected: &str, v: &Value) -> Result<bool, String> {
    let ok = match expected {
        "int" => matches!(v, Value::Int(_) | Value::BigInt(_)),
        "float" => matches!(v, Value::Float(_)),
        "string" => matches!(v, Value::String(_)),
        "bool" => matches!(v, Value::Bool(_)),
        "nil" => matches!(v, Value::Nil),
        "array" => matches!(
            v,
            Value::Array(_)
                | Value::IntArray(_)
                | Value::FloatArray(_)
                | Value::BoolArray(_)
                | Value::ByteArray(_)
                | Value::StringArray(_)
        ),
        "object" => matches!(v, Value::Object(_)),
        "function" => matches!(v, Value::Function(_) | Value::NativeFunction(_)),
        other => return Err(format!("unknown type '{other}'")),
    };
    Ok(ok)
}

fn rule_bool(rule: &HashMap<String, ValueRef>, key: &str) -> bool {
    matches!(
        rule.get(key).map(|v| v.borrow().clone()),
        Some(Value::Bool(true))
    )
}

fn rule_str(rule: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    rule.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

/// Apply a simple rule (`required`, `type`) to one field; append to `errors`.
fn apply_rule(
    field: &str,
    value: Option<&ValueRef>,
    rule: &HashMap<String, ValueRef>,
    errors: &mut Vec<String>,
    span: Span,
) -> NiaoResult<()> {
    let required = rule_bool(rule, "required");
    let missing = match value {
        None => true,
        Some(v) => matches!(&*v.borrow(), Value::Nil),
    };
    if missing {
        if required {
            errors.push(format!("{field}: is required"));
        }
        return Ok(());
    }
    let value = value.unwrap();
    let v = value.borrow();

    if let Some(expected) = rule_str(rule, "type") {
        match type_matches(&expected, &v) {
            Ok(true) => {}
            Ok(false) => {
                errors.push(format!(
                    "{field}: expected {expected}, got {}",
                    v.type_name()
                ));
            }
            Err(msg) => {
                return Err(type_err(span, format!("{field}: {msg}")));
            }
        }
    }
    Ok(())
}

fn invariant_impl(
    obj: &ValueRef,
    rules: &HashMap<String, ValueRef>,
    span: Span,
) -> NiaoResult<Vec<String>> {
    let mut errors = Vec::new();
    let fields = match &*obj.borrow() {
        Value::Object(map) => map.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "ncontract_invariant() expects an object as argument 1, got {}",
                    other.type_name()
                ),
            ))
        }
    };

    let mut names: Vec<&String> = rules.keys().collect();
    names.sort();
    for name in names {
        let rule_val = rules.get(name).unwrap();
        let rule = match &*rule_val.borrow() {
            Value::Object(r) => r.clone(),
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "rule for '{name}' must be an object, got {}",
                        other.type_name()
                    ),
                ))
            }
        };
        apply_rule(name, fields.get(name), &rule, &mut errors, span)?;
    }
    Ok(errors)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// ncontract_require(cond, msg?) — throw RuntimeError if condition is false.
fn ncontract_require(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncontract_require", span)?;
    if cond_ok(&args[0].borrow()) {
        return bool_true();
    }
    let msg = optional_msg(args, 1).unwrap_or_else(|| "precondition failed".to_string());
    Err(contract_err(span, msg))
}

/// ncontract_ensure(cond, msg?) — throw RuntimeError if condition is false.
fn ncontract_ensure(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncontract_ensure", span)?;
    if cond_ok(&args[0].borrow()) {
        return bool_true();
    }
    let msg = optional_msg(args, 1).unwrap_or_else(|| "postcondition failed".to_string());
    Err(contract_err(span, msg))
}

/// ncontract_check(cond, msg?) → true or catchable error_value.
fn ncontract_check(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncontract_check", span)?;
    if cond_ok(&args[0].borrow()) {
        return bool_true();
    }
    let msg = optional_msg(args, 1).unwrap_or_else(|| "check failed".to_string());
    Ok(error_value(
        codes::E3081_NCONTRACT_ERROR,
        "ncontract_error",
        msg,
        span,
    ))
}

/// ncontract_assert_type(v, type_str) — throw if type does not match.
fn ncontract_assert_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncontract_assert_type", span)?;
    let type_str = match &*args[1].borrow() {
        Value::String(s) => s.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "ncontract_assert_type() expects a type string as argument 2, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    if !KNOWN_TYPES.contains(&type_str.as_str()) {
        return Err(type_err(
            span,
            format!(
                "ncontract_assert_type() unknown type '{type_str}' (expected one of: {})",
                KNOWN_TYPES.join("|")
            ),
        ));
    }
    let v = args[0].borrow();
    match type_matches(&type_str, &v) {
        Ok(true) => Ok(Rc::clone(&args[0])),
        Ok(false) => Err(type_err(
            span,
            format!(
                "assert_type failed: expected {type_str}, got {}",
                v.type_name()
            ),
        )),
        Err(msg) => Err(type_err(span, msg)),
    }
}

/// ncontract_invariant(obj, rules) → {ok, errors}
fn ncontract_invariant(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncontract_invariant", span)?;
    let rules = match &*args[1].borrow() {
        Value::Object(map) => map.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "ncontract_invariant() expects a rules object as argument 2, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    let errors = invariant_impl(&args[0], &rules, span)?;
    let mut out = HashMap::new();
    out.insert("ok".to_string(), Value::Bool(errors.is_empty()).ref_cell());
    out.insert(
        "errors".to_string(),
        Value::Array(
            errors
                .into_iter()
                .map(|e| Value::String(e).ref_cell())
                .collect(),
        )
        .ref_cell(),
    );
    Ok(Value::Object(out).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncontract_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncontract_fns![
    ("ncontract_require", "require", ncontract_require),
    ("ncontract_ensure", "ensure", ncontract_ensure),
    ("ncontract_check", "check", ncontract_check),
    (
        "ncontract_assert_type",
        "assert_type",
        ncontract_assert_type
    ),
    ("ncontract_invariant", "invariant", ncontract_invariant),
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

pub const MODULE_NAME: &str = "ncontract";
pub const MODULE_PATHS: &[&str] = &["ncontract", "std/ncontract"];

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
    fn require_passes_and_fails() {
        let ok = ncontract_require(&[Value::Bool(true).ref_cell()], span()).unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(true)));

        let err = ncontract_require(
            &[
                Value::Bool(false).ref_cell(),
                Value::String("need positive".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap_err();
        assert_eq!(err.code(), codes::E3081_NCONTRACT_ERROR);
        match err {
            RuntimeError::Generic { message, .. } => assert_eq!(message, "need positive"),
            other => panic!("expected Generic error, got {other:?}"),
        }
    }

    #[test]
    fn ensure_default_message() {
        let err = ncontract_ensure(&[Value::Nil.ref_cell()], span()).unwrap_err();
        assert_eq!(err.code(), codes::E3081_NCONTRACT_ERROR);
        match err {
            RuntimeError::Generic { message, .. } => {
                assert_eq!(message, "postcondition failed")
            }
            other => panic!("expected Generic error, got {other:?}"),
        }
    }

    #[test]
    fn check_returns_error_value() {
        let ok = ncontract_check(&[Value::Bool(true).ref_cell()], span()).unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(true)));

        let bad = ncontract_check(
            &[
                Value::Bool(false).ref_cell(),
                Value::String("nope".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let bad_ref = bad.borrow();
        let (code, msg) = match &*bad_ref {
            Value::Error(e) => (e.code, e.message.clone()),
            other => panic!("expected error value, got {other:?}"),
        };
        drop(bad_ref);
        assert_eq!(code, codes::E3081_NCONTRACT_ERROR);
        assert!(msg.contains("nope"));
    }

    #[test]
    fn assert_type_int_and_mismatch() {
        let v = Value::Int(42).ref_cell();
        let got =
            ncontract_assert_type(&[v.clone(), Value::String("int".into()).ref_cell()], span())
                .unwrap();
        assert!(matches!(&*got.borrow(), Value::Int(42)));

        let err = ncontract_assert_type(
            &[
                Value::String("x".into()).ref_cell(),
                Value::String("int".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap_err();
        assert_eq!(err.code(), codes::E3082_NCONTRACT_TYPE);
    }

    #[test]
    fn assert_type_rejects_unknown() {
        let err = ncontract_assert_type(
            &[
                Value::Int(1).ref_cell(),
                Value::String("number".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap_err();
        assert_eq!(err.code(), codes::E3082_NCONTRACT_TYPE);
        match err {
            RuntimeError::Generic { message, .. } => assert!(message.contains("unknown type")),
            other => panic!("expected Generic error, got {other:?}"),
        }
    }

    #[test]
    fn invariant_required_and_type() {
        let mut fields = HashMap::new();
        fields.insert("age".to_string(), Value::String("old".into()).ref_cell());
        let obj = Value::Object(fields).ref_cell();

        let mut name_rule = HashMap::new();
        name_rule.insert("required".to_string(), Value::Bool(true).ref_cell());
        let mut age_rule = HashMap::new();
        age_rule.insert("type".to_string(), Value::String("int".into()).ref_cell());
        let mut rules = HashMap::new();
        rules.insert("name".to_string(), Value::Object(name_rule).ref_cell());
        rules.insert("age".to_string(), Value::Object(age_rule).ref_cell());

        let result = ncontract_invariant(&[obj, Value::Object(rules).ref_cell()], span()).unwrap();
        let result_ref = result.borrow();
        let (ok, err_len) = match &*result_ref {
            Value::Object(map) => {
                let ok_v = map.get("ok").unwrap().borrow();
                let ok = matches!(&*ok_v, Value::Bool(false));
                let errs_v = map.get("errors").unwrap().borrow();
                let err_len = match &*errs_v {
                    Value::Array(errs) => errs.len(),
                    other => panic!("expected errors array, got {other:?}"),
                };
                (ok, err_len)
            }
            other => panic!("expected object, got {other:?}"),
        };
        drop(result_ref);
        assert!(ok);
        assert_eq!(err_len, 2);
    }

    #[test]
    fn invariant_passes() {
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), Value::String("vivek".into()).ref_cell());
        fields.insert("age".to_string(), Value::Int(27).ref_cell());
        let obj = Value::Object(fields).ref_cell();

        let mut name_rule = HashMap::new();
        name_rule.insert("required".to_string(), Value::Bool(true).ref_cell());
        name_rule.insert(
            "type".to_string(),
            Value::String("string".into()).ref_cell(),
        );
        let mut age_rule = HashMap::new();
        age_rule.insert("type".to_string(), Value::String("int".into()).ref_cell());
        let mut rules = HashMap::new();
        rules.insert("name".to_string(), Value::Object(name_rule).ref_cell());
        rules.insert("age".to_string(), Value::Object(age_rule).ref_cell());

        let result = ncontract_invariant(&[obj, Value::Object(rules).ref_cell()], span()).unwrap();
        let result_ref = result.borrow();
        let (ok, err_len) = match &*result_ref {
            Value::Object(map) => {
                let ok_v = map.get("ok").unwrap().borrow();
                let ok = matches!(&*ok_v, Value::Bool(true));
                let errs_v = map.get("errors").unwrap().borrow();
                let err_len = match &*errs_v {
                    Value::Array(errs) => errs.len(),
                    other => panic!("expected errors array, got {other:?}"),
                };
                (ok, err_len)
            }
            other => panic!("expected object, got {other:?}"),
        };
        drop(result_ref);
        assert!(ok);
        assert_eq!(err_len, 0);
    }

    #[test]
    fn arity_errors() {
        let err = ncontract_require(&[], span()).unwrap_err();
        assert_eq!(err.code(), codes::E3080_NCONTRACT_ARITY);
    }
}
