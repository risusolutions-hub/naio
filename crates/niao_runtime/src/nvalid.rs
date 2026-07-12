//! Native nvalid standard library — declarative data validation:
//! schema objects with type/required/range/length/pattern/one_of rules,
//! plus fast built-in checks (email, url, uuid, ipv4). Returns friendly
//! `{ok, errors}` results; `assert_valid` throws on failure.
//!
//! Import with `import "nvalid"` (or `import "std/nvalid"`).

use crate::{error_value, values_equal, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_regex::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

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
            codes::E2680_NVALID_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
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

fn schema_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E2682_NVALID_SCHEMA, msg.into())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

// ---------------------------------------------------------------------------
// Built-in string checks (hand-rolled — no regex allocation)
// ---------------------------------------------------------------------------

fn check_email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    if local.is_empty() || local.len() > 64 || domain.is_empty() || domain.len() > 255 {
        return false;
    }
    if s.contains(' ') || s.contains("..") {
        return false;
    }
    let local_ok = local.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-')
    }) && !local.starts_with('.')
        && !local.ends_with('.');
    if !local_ok {
        return false;
    }
    let mut labels = domain.split('.');
    let mut count = 0;
    let labels_ok = labels.all(|label| {
        count += 1;
        !label.is_empty()
            && label.len() <= 63
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    });
    labels_ok && count >= 2
}

fn check_url(s: &str) -> bool {
    let rest = if let Some(r) = s.strip_prefix("https://") {
        r
    } else if let Some(r) = s.strip_prefix("http://") {
        r
    } else if let Some(r) = s.strip_prefix("ftp://") {
        r
    } else if let Some(r) = s.strip_prefix("ws://") {
        r
    } else if let Some(r) = s.strip_prefix("wss://") {
        r
    } else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.split('@').next_back().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    !host.is_empty()
        && !s.contains(' ')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '[' | ']' | ':'))
}

fn check_uuid(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (i, b) in bytes.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if *b != b'-' {
                    return false;
                }
            }
            _ => {
                if !b.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

fn check_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| {
        !p.is_empty()
            && p.len() <= 3
            && p.chars().all(|c| c.is_ascii_digit())
            && (p.len() == 1 || !p.starts_with('0'))
            && p.parse::<u32>().map(|n| n <= 255).unwrap_or(false)
    })
}

// ---------------------------------------------------------------------------
// Pattern cache (compiled niao_regex, thread-local)
// ---------------------------------------------------------------------------

thread_local! {
    static PATTERN_CACHE: RefCell<HashMap<String, Rc<Regex>>> = RefCell::new(HashMap::new());
}

const PATTERN_CACHE_CAP: usize = 128;

fn compiled(pattern: &str, span: Span) -> NiaoResult<Rc<Regex>> {
    PATTERN_CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        if let Some(re) = map.get(pattern) {
            return Ok(Rc::clone(re));
        }
        let re = Regex::new(pattern)
            .map_err(|e| schema_err(span, format!("invalid pattern '{pattern}': {e}")))?;
        if map.len() >= PATTERN_CACHE_CAP {
            if let Some(k) = map.keys().next().cloned() {
                map.remove(&k);
            }
        }
        let re = Rc::new(re);
        map.insert(pattern.to_string(), Rc::clone(&re));
        Ok(re)
    })
}

// ---------------------------------------------------------------------------
// Rule engine
// ---------------------------------------------------------------------------

fn type_matches(expected: &str, v: &Value) -> Result<bool, String> {
    let ok = match expected {
        "string" | "str" => matches!(v, Value::String(_)),
        "int" => matches!(v, Value::Int(_) | Value::BigInt(_)),
        "float" => matches!(v, Value::Float(_)),
        "number" => matches!(v, Value::Int(_) | Value::Float(_) | Value::BigInt(_)),
        "bool" => matches!(v, Value::Bool(_)),
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
        "nil" => matches!(v, Value::Nil),
        other => return Err(format!("unknown type '{other}' in schema")),
    };
    Ok(ok)
}

fn value_len(v: &Value) -> Option<i64> {
    match v {
        Value::String(s) => Some(s.chars().count() as i64),
        Value::Array(items) => Some(items.len() as i64),
        Value::IntArray(items) => Some(items.len() as i64),
        Value::FloatArray(items) => Some(items.len() as i64),
        Value::BoolArray(items) => Some(items.len() as i64),
        Value::ByteArray(items) => Some(items.len() as i64),
        Value::StringArray(items) => Some(items.len() as i64),
        Value::Object(map) => Some(map.len() as i64),
        _ => None,
    }
}

fn as_num(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

fn rule_num(rule: &HashMap<String, ValueRef>, key: &str) -> Option<f64> {
    rule.get(key).and_then(|v| as_num(&v.borrow()))
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

/// Validate one value against one rule object; append messages to `errors`.
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
                return Ok(());
            }
            Err(msg) => return Err(schema_err(span, format!("{field}: {msg}"))),
        }
    }

    // Numeric range
    if let Some(min) = rule_num(rule, "min") {
        if let Some(n) = as_num(&v) {
            if n < min {
                errors.push(format!("{field}: must be >= {min}"));
            }
        }
    }
    if let Some(max) = rule_num(rule, "max") {
        if let Some(n) = as_num(&v) {
            if n > max {
                errors.push(format!("{field}: must be <= {max}"));
            }
        }
    }

    // Length bounds
    if let Some(min_len) = rule_num(rule, "min_len") {
        match value_len(&v) {
            Some(len) if (len as f64) < min_len => {
                errors.push(format!("{field}: length must be >= {min_len}"));
            }
            _ => {}
        }
    }
    if let Some(max_len) = rule_num(rule, "max_len") {
        match value_len(&v) {
            Some(len) if (len as f64) > max_len => {
                errors.push(format!("{field}: length must be <= {max_len}"));
            }
            _ => {}
        }
    }

    // one_of
    if let Some(options) = rule.get("one_of") {
        if let Value::Array(opts) = &*options.borrow() {
            let found = opts.iter().any(|opt| values_equal(&opt.borrow(), &v));
            if !found {
                let rendered: Vec<String> = opts.iter().map(|o| o.borrow().to_string()).collect();
                errors.push(format!("{field}: must be one of [{}]", rendered.join(", ")));
            }
        } else {
            return Err(schema_err(span, format!("{field}: one_of must be an array")));
        }
    }

    // String-only checks
    if let Value::String(s) = &*v {
        if let Some(pattern) = rule_str(rule, "pattern") {
            let re = compiled(&pattern, span)?;
            if !re.is_match(s) {
                errors.push(format!("{field}: does not match pattern"));
            }
        }
        if rule_bool(rule, "email") && !check_email(s) {
            errors.push(format!("{field}: must be a valid email"));
        }
        if rule_bool(rule, "url") && !check_url(s) {
            errors.push(format!("{field}: must be a valid URL"));
        }
        if rule_bool(rule, "uuid") && !check_uuid(s) {
            errors.push(format!("{field}: must be a valid UUID"));
        }
        if rule_bool(rule, "ipv4") && !check_ipv4(s) {
            errors.push(format!("{field}: must be a valid IPv4 address"));
        }
        if rule_bool(rule, "non_blank") && s.trim().is_empty() {
            errors.push(format!("{field}: must not be blank"));
        }
    }
    Ok(())
}

fn validate_impl(
    value: &ValueRef,
    schema: &HashMap<String, ValueRef>,
    span: Span,
) -> NiaoResult<Vec<String>> {
    let mut errors = Vec::new();
    let target = value.borrow();
    match &*target {
        Value::Object(fields) => {
            let mut names: Vec<&String> = schema.keys().collect();
            names.sort();
            for name in names {
                let rule_val = schema.get(name).unwrap();
                let rule = match &*rule_val.borrow() {
                    Value::Object(r) => r.clone(),
                    other => {
                        return Err(schema_err(
                            span,
                            format!("rule for '{name}' must be an object, got {}", other.type_name()),
                        ))
                    }
                };
                apply_rule(name, fields.get(name), &rule, &mut errors, span)?;
            }
        }
        _ => {
            // Non-object target: schema is a single rule object applied to "value".
            apply_rule("value", Some(value), schema, &mut errors, span)?;
        }
    }
    Ok(errors)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn schema_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a schema object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

/// nvalid_check(value, schema) → {ok, errors}
fn nvalid_check(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nvalid_check", span)?;
    let schema = schema_arg(args, 1, "nvalid_check", span)?;
    let errors = validate_impl(&args[0], &schema, span)?;
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

/// nvalid_assert(value, schema) → value on success, catchable error on failure.
fn nvalid_assert(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nvalid_assert", span)?;
    let schema = schema_arg(args, 1, "nvalid_assert", span)?;
    let errors = validate_impl(&args[0], &schema, span)?;
    if errors.is_empty() {
        Ok(Rc::clone(&args[0]))
    } else {
        Ok(error_value(
            codes::E2681_NVALID_ERROR,
            "nvalid_error",
            format!("validation failed: {}", errors.join("; ")),
            span,
        ))
    }
}

fn nvalid_is_email(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nvalid_is_email", span)?;
    let s = string_arg(args, 0, "nvalid_is_email", span)?;
    bool_val(check_email(&s))
}

fn nvalid_is_url(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nvalid_is_url", span)?;
    let s = string_arg(args, 0, "nvalid_is_url", span)?;
    bool_val(check_url(&s))
}

fn nvalid_is_uuid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nvalid_is_uuid", span)?;
    let s = string_arg(args, 0, "nvalid_is_uuid", span)?;
    bool_val(check_uuid(&s))
}

fn nvalid_is_ipv4(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nvalid_is_ipv4", span)?;
    let s = string_arg(args, 0, "nvalid_is_ipv4", span)?;
    bool_val(check_ipv4(&s))
}

fn nvalid_is_int_str(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nvalid_is_int_str", span)?;
    let s = string_arg(args, 0, "nvalid_is_int_str", span)?;
    bool_val(s.parse::<i64>().is_ok())
}

fn nvalid_is_float_str(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nvalid_is_float_str", span)?;
    let s = string_arg(args, 0, "nvalid_is_float_str", span)?;
    bool_val(s.parse::<f64>().is_ok())
}

fn nvalid_matches(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nvalid_matches", span)?;
    let s = string_arg(args, 0, "nvalid_matches", span)?;
    let pattern = string_arg(args, 1, "nvalid_matches", span)?;
    let re = compiled(&pattern, span)?;
    bool_val(re.is_match(&s))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nvalid_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nvalid_fns![
    ("nvalid_check", "check", nvalid_check),
    ("nvalid_assert", "assert", nvalid_assert),
    ("nvalid_is_email", "is_email", nvalid_is_email),
    ("nvalid_is_url", "is_url", nvalid_is_url),
    ("nvalid_is_uuid", "is_uuid", nvalid_is_uuid),
    ("nvalid_is_ipv4", "is_ipv4", nvalid_is_ipv4),
    ("nvalid_is_int_str", "is_int_str", nvalid_is_int_str),
    ("nvalid_is_float_str", "is_float_str", nvalid_is_float_str),
    ("nvalid_matches", "matches", nvalid_matches),
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

pub const MODULE_NAME: &str = "nvalid";
pub const MODULE_PATHS: &[&str] = &["nvalid", "std/nvalid"];

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
    fn email_checks() {
        assert!(check_email("dev@niao.dev"));
        assert!(check_email("a.b+c@sub.example.com"));
        assert!(!check_email("no-at-sign"));
        assert!(!check_email("a b@example.com"));
        assert!(!check_email("a@nodot"));
        assert!(!check_email("a@-bad.com"));
    }

    #[test]
    fn url_checks() {
        assert!(check_url("https://niao.risu.in/docs?x=1"));
        assert!(check_url("http://localhost:8080/path"));
        assert!(!check_url("notaurl"));
        assert!(!check_url("https://"));
        assert!(!check_url("https://has space.com"));
    }

    #[test]
    fn uuid_and_ipv4() {
        assert!(check_uuid("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!check_uuid("550e8400e29b41d4a716446655440000"));
        assert!(check_ipv4("192.168.0.1"));
        assert!(check_ipv4("0.0.0.0"));
        assert!(!check_ipv4("256.1.1.1"));
        assert!(!check_ipv4("01.1.1.1"));
        assert!(!check_ipv4("1.1.1"));
    }

    #[test]
    fn schema_validation() {
        // value: {name: "x", age: 200}
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), Value::String("x".into()).ref_cell());
        fields.insert("age".to_string(), Value::Int(200).ref_cell());
        let value = Value::Object(fields).ref_cell();

        // schema: {name: {type string, min_len 2}, age: {type int, max 150}, email: {required}}
        let mut name_rule = HashMap::new();
        name_rule.insert("type".to_string(), Value::String("string".into()).ref_cell());
        name_rule.insert("min_len".to_string(), Value::Int(2).ref_cell());
        let mut age_rule = HashMap::new();
        age_rule.insert("type".to_string(), Value::String("int".into()).ref_cell());
        age_rule.insert("max".to_string(), Value::Int(150).ref_cell());
        let mut email_rule = HashMap::new();
        email_rule.insert("required".to_string(), Value::Bool(true).ref_cell());
        let mut schema = HashMap::new();
        schema.insert("name".to_string(), Value::Object(name_rule).ref_cell());
        schema.insert("age".to_string(), Value::Object(age_rule).ref_cell());
        schema.insert("email".to_string(), Value::Object(email_rule).ref_cell());

        let result = nvalid_check(&[value, Value::Object(schema).ref_cell()], span()).unwrap();
        match &*result.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map.get("ok").unwrap().borrow(), Value::Bool(false)));
                match &*map.get("errors").unwrap().borrow() {
                    Value::Array(errs) => assert_eq!(errs.len(), 3),
                    other => panic!("expected array, got {other:?}"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        };
    }
}
