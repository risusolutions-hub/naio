//! Native nshape standard library — describe and check value shapes:
//! `of`/`match` for shape strings, `rank`/`dims` for arrays, and simple
//! `check` against a type name or schema object of type strings.
//!
//! Import with `import "nshape"` (or `import "std/nshape"`).

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::rc::Rc;

// Error codes (wired into codes.rs by central integration).
const E3120_NSHAPE_ARITY: u32 = 3120;
const E3121_NSHAPE_ERROR: u32 = 3121;
const E3122_NSHAPE_TYPE: u32 = 3122;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3120_NSHAPE_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3122_NSHAPE_TYPE, msg.into())
}

fn shape_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3121_NSHAPE_ERROR, msg.into())
}

// ---------------------------------------------------------------------------
// Shape description
// ---------------------------------------------------------------------------

fn is_array_value(v: &Value) -> bool {
    matches!(
        v,
        Value::Array(_)
            | Value::IntArray(_)
            | Value::FloatArray(_)
            | Value::BoolArray(_)
            | Value::ByteArray(_)
            | Value::StringArray(_)
    )
}

fn array_len(v: &Value) -> Option<usize> {
    match v {
        Value::Array(items) => Some(items.len()),
        Value::IntArray(items) => Some(items.len()),
        Value::FloatArray(items) => Some(items.len()),
        Value::BoolArray(items) => Some(items.len()),
        Value::ByteArray(items) => Some(items.len()),
        Value::StringArray(items) => Some(items.len()),
        _ => None,
    }
}

/// Structural shape string for a value.
fn shape_of(v: &Value) -> String {
    match v {
        Value::Int(_) => "int".into(),
        Value::BigInt(_) => "bigint".into(),
        Value::Float(_) => "float".into(),
        Value::String(_) => "string".into(),
        Value::Bool(_) => "bool".into(),
        Value::Nil => "nil".into(),
        Value::IntArray(items) => format!("int_array[{}]", items.len()),
        Value::FloatArray(items) => format!("float_array[{}]", items.len()),
        Value::BoolArray(items) => format!("bool_array[{}]", items.len()),
        Value::ByteArray(items) => format!("byte_array[{}]", items.len()),
        Value::StringArray(items) => format!("string_array[{}]", items.len()),
        Value::Array(items) => format!("array[{}]", items.len()),
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| {
                    let field = map.get(k).unwrap();
                    format!("{}: {}", k, shape_of(&field.borrow()))
                })
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        Value::Instance(inst) => {
            let mut keys: Vec<&String> = inst.fields.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| {
                    let field = inst.fields.get(k).unwrap();
                    format!("{}: {}", k, shape_of(&field.borrow()))
                })
                .collect();
            format!("{} {{{}}}", inst.class_name, parts.join(", "))
        }
        Value::Function(_) | Value::NativeFunction(_) => "function".into(),
        Value::Native(ds) => ds.borrow().kind_name().to_string(),
        Value::Error(_) => "error".into(),
        Value::NclHandle(_) => "ncl_handle".into(),
        Value::NmlHandle(_) => "nml_handle".into(),
        #[cfg(feature = "nmongo")]
        Value::BsonDoc(_) => "object".into(),
    }
}

/// Whether `v` matches a simple expected type name (ignores array lengths).
fn type_name_matches(expected: &str, v: &Value) -> bool {
    match expected {
        "int" => matches!(v, Value::Int(_)),
        "bigint" => matches!(v, Value::BigInt(_)),
        "float" => matches!(v, Value::Float(_)),
        "number" => matches!(v, Value::Int(_) | Value::Float(_) | Value::BigInt(_)),
        "string" | "str" => matches!(v, Value::String(_)),
        "bool" => matches!(v, Value::Bool(_)),
        "nil" => matches!(v, Value::Nil),
        "object" => matches!(v, Value::Object(_)),
        "array" => is_array_value(v),
        "int_array" => matches!(v, Value::IntArray(_)),
        "float_array" => matches!(v, Value::FloatArray(_)),
        "bool_array" => matches!(v, Value::BoolArray(_)),
        "byte_array" => matches!(v, Value::ByteArray(_)),
        "string_array" => matches!(v, Value::StringArray(_)),
        "function" => matches!(v, Value::Function(_) | Value::NativeFunction(_)),
        "error" => matches!(v, Value::Error(_)),
        other => {
            // Exact shape string from of(), e.g. "array[3]" or "float_array[10]"
            shape_of(v) == other
        }
    }
}

fn check_against_string(value: &Value, expected: &str, errors: &mut Vec<String>) {
    if type_name_matches(expected, value) {
        return;
    }
    errors.push(format!("expected {}, got {}", expected, shape_of(value)));
}

fn check_against_schema(
    value: &Value,
    schema: &HashMap<String, ValueRef>,
    errors: &mut Vec<String>,
    span: Span,
) -> NiaoResult<()> {
    let fields = match value {
        Value::Object(map) => map,
        other => {
            errors.push(format!(
                "expected object matching schema, got {}",
                shape_of(other)
            ));
            return Ok(());
        }
    };

    let mut keys: Vec<&String> = schema.keys().collect();
    keys.sort();
    for key in keys {
        let expected_val = schema.get(key).unwrap();
        let expected_type = match &*expected_val.borrow() {
            Value::String(s) => s.clone(),
            other => {
                return Err(shape_err(
                    span,
                    format!(
                        "schema value for '{key}' must be a type string, got {}",
                        other.type_name()
                    ),
                ));
            }
        };
        match fields.get(key) {
            None => errors.push(format!("{key}: missing (expected {expected_type})")),
            Some(actual) => {
                let actual = actual.borrow();
                if !type_name_matches(&expected_type, &actual) {
                    errors.push(format!(
                        "{key}: expected {expected_type}, got {}",
                        shape_of(&actual)
                    ));
                }
            }
        }
    }
    Ok(())
}

fn result_ok_errors(errors: Vec<String>) -> NiaoResult<ValueRef> {
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
// Builtins
// ---------------------------------------------------------------------------

/// nshape_of(value) → shape string
fn nshape_of(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nshape_of", span)?;
    Ok(Value::String(shape_of(&args[0].borrow())).ref_cell())
}

/// nshape_rank(arr) → 1 for arrays, else 0
fn nshape_rank(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nshape_rank", span)?;
    let rank = if is_array_value(&args[0].borrow()) {
        1
    } else {
        0
    };
    Ok(Value::Int(rank).ref_cell())
}

/// nshape_dims(arr) → [len] for arrays, else []
fn nshape_dims(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nshape_dims", span)?;
    match array_len(&args[0].borrow()) {
        Some(len) => Ok(Value::Array(vec![Value::Int(len as i64).ref_cell()]).ref_cell()),
        None => Ok(Value::Array(Vec::new()).ref_cell()),
    }
}

/// nshape_match(a, b) → true when of(a) == of(b)
fn nshape_match(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nshape_match", span)?;
    let a = shape_of(&args[0].borrow());
    let b = shape_of(&args[1].borrow());
    Ok(Value::Bool(a == b).ref_cell())
}

/// nshape_check(value, expected) → {ok, errors}
/// `expected` is a type/shape string or a schema object of type strings.
fn nshape_check(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nshape_check", span)?;
    let value = args[0].borrow();
    let mut errors = Vec::new();
    match &*args[1].borrow() {
        Value::String(expected) => {
            check_against_string(&value, expected, &mut errors);
        }
        Value::Object(schema) => {
            check_against_schema(&value, schema, &mut errors, span)?;
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "nshape_check() expects a type string or schema object as argument 2, got {}",
                    other.type_name()
                ),
            ));
        }
    }
    result_ok_errors(errors)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nshape_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nshape_fns![
    ("nshape_of", "of", nshape_of),
    ("nshape_rank", "rank", nshape_rank),
    ("nshape_dims", "dims", nshape_dims),
    ("nshape_match", "match", nshape_match),
    ("nshape_check", "check", nshape_check),
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

pub const MODULE_NAME: &str = "nshape";
pub const MODULE_PATHS: &[&str] = &["nshape", "std/nshape"];

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
    fn of_scalars_and_arrays() {
        assert_eq!(shape_of(&Value::Int(42)), "int");
        assert_eq!(shape_of(&Value::Float(1.5)), "float");
        assert_eq!(shape_of(&Value::String("x".into())), "string");
        assert_eq!(shape_of(&Value::Bool(true)), "bool");
        assert_eq!(shape_of(&Value::Nil), "nil");
        assert_eq!(
            shape_of(&Value::Array(vec![
                Value::Int(1).ref_cell(),
                Value::Int(2).ref_cell(),
                Value::Int(3).ref_cell(),
            ])),
            "array[3]"
        );
        assert_eq!(
            shape_of(&Value::FloatArray(vec![0.0; 10])),
            "float_array[10]"
        );
        assert_eq!(shape_of(&Value::IntArray(vec![1, 2])), "int_array[2]");
    }

    #[test]
    fn of_object_sorted_keys() {
        let mut map = HashMap::new();
        map.insert("age".to_string(), Value::Int(27).ref_cell());
        map.insert("name".to_string(), Value::String("vivek".into()).ref_cell());
        assert_eq!(shape_of(&Value::Object(map)), "{age: int, name: string}");
    }

    fn ok_flag(result: &ValueRef) -> bool {
        match &*result.borrow() {
            Value::Object(map) => matches!(&*map.get("ok").unwrap().borrow(), Value::Bool(true)),
            _ => false,
        }
    }

    fn error_count(result: &ValueRef) -> usize {
        match &*result.borrow() {
            Value::Object(map) => match &*map.get("errors").unwrap().borrow() {
                Value::Array(errs) => errs.len(),
                _ => 0,
            },
            _ => 0,
        }
    }

    #[test]
    fn rank_and_dims() {
        let arr = Value::Array(vec![Value::Int(1).ref_cell()]).ref_cell();
        let r = nshape_rank(&[Rc::clone(&arr)], span()).unwrap();
        assert!(matches!(&*r.borrow(), Value::Int(1)));
        let d = nshape_dims(&[arr], span()).unwrap();
        match &*d.borrow() {
            Value::Array(items) => {
                assert_eq!(items.len(), 1);
                assert!(matches!(&*items[0].borrow(), Value::Int(1)));
            }
            other => panic!("expected dims array, got {other:?}"),
        };

        let scalar = Value::Int(9).ref_cell();
        let r0 = nshape_rank(&[Rc::clone(&scalar)], span()).unwrap();
        assert!(matches!(&*r0.borrow(), Value::Int(0)));
        let d0 = nshape_dims(&[scalar], span()).unwrap();
        match &*d0.borrow() {
            Value::Array(items) => assert!(items.is_empty()),
            other => panic!("expected empty dims, got {other:?}"),
        };
    }

    #[test]
    fn match_compares_shapes() {
        let a = Value::Array(vec![Value::Int(1).ref_cell(), Value::Int(2).ref_cell()]).ref_cell();
        let b = Value::Array(vec![
            Value::String("x".into()).ref_cell(),
            Value::String("y".into()).ref_cell(),
        ])
        .ref_cell();
        let same = nshape_match(&[Rc::clone(&a), b], span()).unwrap();
        assert!(matches!(&*same.borrow(), Value::Bool(true)));

        let c = Value::Array(vec![Value::Int(1).ref_cell()]).ref_cell();
        let diff = nshape_match(&[a, c], span()).unwrap();
        assert!(matches!(&*diff.borrow(), Value::Bool(false)));
    }

    #[test]
    fn check_type_string() {
        let v = Value::Int(3).ref_cell();
        let ok = nshape_check(
            &[Rc::clone(&v), Value::String("int".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert!(ok_flag(&ok));

        let bad = nshape_check(&[v, Value::String("string".into()).ref_cell()], span()).unwrap();
        assert!(!ok_flag(&bad));
        assert_eq!(error_count(&bad), 1);
    }

    #[test]
    fn check_schema_object() {
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), Value::String("x".into()).ref_cell());
        fields.insert("age".to_string(), Value::Int(20).ref_cell());
        let value = Value::Object(fields).ref_cell();

        let mut schema = HashMap::new();
        schema.insert(
            "name".to_string(),
            Value::String("string".into()).ref_cell(),
        );
        schema.insert("age".to_string(), Value::String("int".into()).ref_cell());
        schema.insert(
            "email".to_string(),
            Value::String("string".into()).ref_cell(),
        );

        let result = nshape_check(&[value, Value::Object(schema).ref_cell()], span()).unwrap();
        assert!(!ok_flag(&result));
        assert_eq!(error_count(&result), 1);
        match &*result.borrow() {
            Value::Object(map) => match &*map.get("errors").unwrap().borrow() {
                Value::Array(errs) => {
                    assert!(errs[0].borrow().to_string().contains("email"));
                }
                other => panic!("expected errors array, got {other:?}"),
            },
            other => panic!("expected object, got {other:?}"),
        };
    }

    #[test]
    fn check_array_type_name() {
        let arr = Value::IntArray(vec![1, 2, 3]).ref_cell();
        let r = nshape_check(
            &[Rc::clone(&arr), Value::String("array".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert!(ok_flag(&r));

        let exact = nshape_check(
            &[arr, Value::String("int_array[3]".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert!(ok_flag(&exact));
    }

    #[test]
    fn arity_errors() {
        let err = nshape_of(&[], span()).unwrap_err();
        match err {
            RuntimeError::Generic { code, .. } => assert_eq!(code, E3120_NSHAPE_ARITY),
            other => panic!("expected Generic error, got {other:?}"),
        }
    }
}
