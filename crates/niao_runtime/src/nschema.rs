//! Native nschema standard library — infer JSON Schema from examples,
//! validate/coerce/parse structured data, and emit LLM prompt snippets /
//! tool specs (pairs with `nagent` for agent tool wiring).
//!
//! Import with `import "nschema"` (or `import "std/nschema"`).

use crate::{error_value, json_parse, json_stringify, values_equal, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_regex::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3290_NSCHEMA_ARITY: u32 = 3290;
const E3291_NSCHEMA_ERROR: u32 = 3291;
const E3292_NSCHEMA_TYPE: u32 = 3292;
const E3293_NSCHEMA_VALIDATE: u32 = 3293;

thread_local! {
    static PATTERN_CACHE: RefCell<HashMap<String, Rc<Regex>>> = RefCell::new(HashMap::new());
}

const PATTERN_CACHE_CAP: usize = 64;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3292_NSCHEMA_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3290_NSCHEMA_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3290_NSCHEMA_ARITY,
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

fn schema_obj(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
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

fn schema_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3293_NSCHEMA_VALIDATE, "nschema_error", msg.into(), span)
}

fn nschema_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3291_NSCHEMA_ERROR, "nschema_error", msg.into(), span)
}

fn compiled(pattern: &str, span: Span) -> NiaoResult<Rc<Regex>> {
    PATTERN_CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        if let Some(re) = map.get(pattern) {
            return Ok(Rc::clone(re));
        }
        let re = Regex::new(pattern).map_err(|e| {
            RuntimeError::at(span, E3291_NSCHEMA_ERROR, format!("invalid pattern '{pattern}': {e}"))
        })?;
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
// Schema inference
// ---------------------------------------------------------------------------

fn infer_schema(val: &Value) -> HashMap<String, ValueRef> {
    let mut schema = HashMap::new();
    match val {
        Value::Nil => {
            schema.insert("type".into(), Value::String("null".into()).ref_cell());
        }
        Value::Bool(_) => {
            schema.insert("type".into(), Value::String("boolean".into()).ref_cell());
        }
        Value::Int(_) | Value::BigInt(_) => {
            schema.insert("type".into(), Value::String("integer".into()).ref_cell());
        }
        Value::Float(_) => {
            schema.insert("type".into(), Value::String("number".into()).ref_cell());
        }
        Value::String(s) => {
            if s.parse::<i64>().is_ok() {
                schema.insert("type".into(), Value::String("integer".into()).ref_cell());
            } else if s.parse::<f64>().is_ok() {
                schema.insert("type".into(), Value::String("number".into()).ref_cell());
            } else {
                schema.insert("type".into(), Value::String("string".into()).ref_cell());
            }
        }
        Value::Array(items) => {
            schema.insert("type".into(), Value::String("array".into()).ref_cell());
            if let Some(first) = items.first() {
                schema.insert(
                    "items".into(),
                    Value::Object(infer_schema(&first.borrow())).ref_cell(),
                );
            }
        }
        Value::IntArray(_) => {
            schema.insert("type".into(), Value::String("array".into()).ref_cell());
            schema.insert(
                "items".into(),
                Value::Object(HashMap::from([(
                    "type".into(),
                    Value::String("integer".into()).ref_cell(),
                )]))
                .ref_cell(),
            );
        }
        Value::FloatArray(_) => {
            schema.insert("type".into(), Value::String("array".into()).ref_cell());
            schema.insert(
                "items".into(),
                Value::Object(HashMap::from([(
                    "type".into(),
                    Value::String("number".into()).ref_cell(),
                )]))
                .ref_cell(),
            );
        }
        Value::BoolArray(_) => {
            schema.insert("type".into(), Value::String("array".into()).ref_cell());
            schema.insert(
                "items".into(),
                Value::Object(HashMap::from([(
                    "type".into(),
                    Value::String("boolean".into()).ref_cell(),
                )]))
                .ref_cell(),
            );
        }
        Value::Object(map) => {
            schema.insert("type".into(), Value::String("object".into()).ref_cell());
            let mut props = HashMap::new();
            let mut required = Vec::new();
            for (k, v) in map {
                props.insert(k.clone(), Value::Object(infer_schema(&v.borrow())).ref_cell());
                required.push(Value::String(k.clone()).ref_cell());
            }
            schema.insert("properties".into(), Value::Object(props).ref_cell());
            if !required.is_empty() {
                schema.insert("required".into(), Value::Array(required).ref_cell());
            }
        }
        _ => {
            schema.insert("type".into(), Value::String("string".into()).ref_cell());
        }
    }
    schema
}

fn schema_type(schema: &HashMap<String, ValueRef>) -> Option<String> {
    schema.get("type").and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn schema_properties(schema: &HashMap<String, ValueRef>) -> Option<HashMap<String, ValueRef>> {
    schema.get("properties").and_then(|v| match &*v.borrow() {
        Value::Object(m) => Some(m.clone()),
        _ => None,
    })
}

fn schema_items(schema: &HashMap<String, ValueRef>) -> Option<HashMap<String, ValueRef>> {
    schema.get("items").and_then(|v| match &*v.borrow() {
        Value::Object(m) => Some(m.clone()),
        _ => None,
    })
}

fn is_required(schema: &HashMap<String, ValueRef>, key: &str) -> bool {
    schema
        .get("required")
        .and_then(|v| match &*v.borrow() {
            Value::Array(items) => Some(
                items
                    .iter()
                    .any(|item| matches!(&*item.borrow(), Value::String(s) if s == key)),
            ),
            _ => None,
        })
        .unwrap_or(false)
}

fn value_matches_type(val: &Value, ty: &str) -> bool {
    match ty {
        "null" => matches!(val, Value::Nil),
        "boolean" | "bool" => matches!(val, Value::Bool(_)),
        "integer" | "int" => matches!(val, Value::Int(_) | Value::BigInt(_)),
        "number" => matches!(val, Value::Int(_) | Value::Float(_) | Value::BigInt(_)),
        "string" | "str" => matches!(val, Value::String(_)),
        "array" => matches!(
            val,
            Value::Array(_)
                | Value::IntArray(_)
                | Value::FloatArray(_)
                | Value::BoolArray(_)
        ),
        "object" => matches!(val, Value::Object(_)),
        _ => true,
    }
}

fn num_value(val: &Value) -> Option<f64> {
    match val {
        Value::Int(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        Value::BigInt(b) => b.to_string().parse().ok(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn validate_value(
    val: &ValueRef,
    schema: &HashMap<String, ValueRef>,
    path: &str,
    span: Span,
) -> NiaoResult<Vec<String>> {
    let borrowed = val.borrow();
    let mut errors = Vec::new();

    if let Some(ty) = schema_type(schema) {
        if !value_matches_type(&borrowed, &ty) {
            errors.push(format!(
                "{path}: expected type '{ty}', got {}",
                borrowed.type_name()
            ));
            return Ok(errors);
        }
    }

    if let Some(min) = schema.get("min").and_then(|v| num_value(&v.borrow())) {
        if let Some(n) = num_value(&borrowed) {
            if n < min {
                errors.push(format!("{path}: value {n} below min {min}"));
            }
        }
    }
    if let Some(max) = schema.get("max").and_then(|v| num_value(&v.borrow())) {
        if let Some(n) = num_value(&borrowed) {
            if n > max {
                errors.push(format!("{path}: value {n} above max {max}"));
            }
        }
    }

    if let Value::String(s) = &*borrowed {
        if let Some(min_len) = schema.get("min_len").and_then(|v| match &*v.borrow() {
            Value::Int(n) => Some(*n as usize),
            _ => None,
        }) {
            if s.chars().count() < min_len {
                errors.push(format!("{path}: string shorter than min_len {min_len}"));
            }
        }
        if let Some(max_len) = schema.get("max_len").and_then(|v| match &*v.borrow() {
            Value::Int(n) => Some(*n as usize),
            _ => None,
        }) {
            if s.chars().count() > max_len {
                errors.push(format!("{path}: string longer than max_len {max_len}"));
            }
        }
        if let Some(pat) = schema.get("pattern").and_then(|v| match &*v.borrow() {
            Value::String(s) => Some(s.clone()),
            _ => None,
        }) {
            let re = compiled(&pat, span)?;
            if !re.is_match(s) {
                errors.push(format!("{path}: does not match pattern"));
            }
        }
    }

    if let Some(one_of) = schema.get("one_of") {
        match &*one_of.borrow() {
            Value::Array(options) => {
                let ok = options
                    .iter()
                    .any(|opt| values_equal(&borrowed, &opt.borrow()));
                if !ok {
                    errors.push(format!("{path}: not in one_of list"));
                }
            }
            _ => {}
        }
    }

    match &*borrowed {
        Value::Object(map) => {
            if let Some(props) = schema_properties(schema) {
                for (key, rule_ref) in &props {
                    let rule = match &*rule_ref.borrow() {
                        Value::Object(m) => m.clone(),
                        _ => continue,
                    };
                    let child_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    match map.get(key) {
                        None => {
                            if is_required(schema, key)
                                || rule.get("required").is_some_and(|r| {
                                    matches!(&*r.borrow(), Value::Bool(true))
                                })
                            {
                                errors.push(format!("{child_path}: required field missing"));
                            }
                        }
                        Some(v) if matches!(&*v.borrow(), Value::Nil) => {
                            if is_required(schema, key)
                                || rule.get("required").is_some_and(|r| {
                                    matches!(&*r.borrow(), Value::Bool(true))
                                })
                            {
                                errors.push(format!("{child_path}: required field missing"));
                            }
                        }
                        Some(v) => {
                            errors.extend(validate_value(v, &rule, &child_path, span)?);
                        }
                    }
                }
            }
        }
        Value::Array(items) => {
            if let Some(item_schema) = schema_items(schema) {
                for (i, item) in items.iter().enumerate() {
                    let child_path = format!("{path}[{i}]");
                    errors.extend(validate_value(item, &item_schema, &child_path, span)?);
                }
            }
        }
        _ => {}
    }

    Ok(errors)
}

fn coerce_value(val: &ValueRef, schema: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<ValueRef> {
    let borrowed = val.borrow();
    let ty = schema_type(schema);

    let coerced = match (&*borrowed, ty.as_deref()) {
        (Value::String(s), Some("integer" | "int")) => {
            if let Ok(n) = s.parse::<i64>() {
                Value::Int(n).ref_cell()
            } else {
                return Ok(schema_err(span, format!("cannot coerce '{s}' to integer")));
            }
        }
        (Value::String(s), Some("number")) => {
            if let Ok(n) = s.parse::<f64>() {
                Value::Float(n).ref_cell()
            } else {
                return Ok(schema_err(span, format!("cannot coerce '{s}' to number")));
            }
        }
        (Value::String(s), Some("boolean" | "bool")) => {
            match s.to_lowercase().as_str() {
                "true" | "1" | "yes" => Value::Bool(true).ref_cell(),
                "false" | "0" | "no" => Value::Bool(false).ref_cell(),
                _ => return Ok(schema_err(span, format!("cannot coerce '{s}' to boolean"))),
            }
        }
        (Value::Int(n), Some("number")) => Value::Float(*n as f64).ref_cell(),
        (Value::Object(map), Some("object")) => {
            let mut out = HashMap::new();
            if let Some(props) = schema_properties(schema) {
                for (key, rule_ref) in props {
                    if let Some(v) = map.get(&key) {
                        let rule = match &*rule_ref.borrow() {
                            Value::Object(m) => m.clone(),
                            _ => HashMap::new(),
                        };
                        out.insert(key, coerce_value(v, &rule, span)?);
                    } else if is_required(schema, &key) {
                        return Ok(schema_err(span, format!("missing required field '{key}'")));
                    }
                }
                for (key, v) in map {
                    if !out.contains_key(key) {
                        out.insert(key.clone(), Rc::clone(v));
                    }
                }
            } else {
                return Ok(Rc::clone(val));
            }
            Value::Object(out).ref_cell()
        }
        (Value::Array(items), Some("array")) => {
            if let Some(item_schema) = schema_items(schema) {
                let out: NiaoResult<Vec<ValueRef>> = items
                    .iter()
                    .map(|item| coerce_value(item, &item_schema, span))
                    .collect();
                Value::Array(out?).ref_cell()
            } else {
                return Ok(Rc::clone(val));
            }
        }
        _ => return Ok(Rc::clone(val)),
    };

    let errors = validate_value(&coerced, schema, "", span)?;
    if errors.is_empty() {
        Ok(coerced)
    } else {
        Ok(schema_err(
            span,
            format!("coerced value still invalid: {}", errors.join("; ")),
        ))
    }
}

fn schema_to_prompt(schema: &HashMap<String, ValueRef>, title: &str, span: Span) -> NiaoResult<String> {
    let json = json_stringify(&[Value::Object(schema.clone()).ref_cell()], span)?;
    let body = match &*json.borrow() {
        Value::String(s) => s.clone(),
        _ => "{}".into(),
    };
    Ok(format!(
        "{title}\n\nRespond with JSON matching this schema (no markdown fences, no commentary):\n{body}"
    ))
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nschema_from_example(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nschema_from_example", span)?;
    Ok(Value::Object(infer_schema(&args[0].borrow())).ref_cell())
}

fn nschema_validate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nschema_validate", span)?;
    let schema = schema_obj(args, 1, "nschema_validate", span)?;
    let errors = validate_value(&args[0], &schema, "", span)?;
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

fn nschema_coerce(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nschema_coerce", span)?;
    let schema = schema_obj(args, 1, "nschema_coerce", span)?;
    coerce_value(&args[0], &schema, span)
}

fn nschema_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nschema_parse", span)?;
    let schema = schema_obj(args, 1, "nschema_parse", span)?;
    let text = string_arg(args, 0, "nschema_parse", span)?;
    let parsed = json_parse(&[Value::String(text).ref_cell()], span)?;
    coerce_value(&parsed, &schema, span)
}

fn nschema_prompt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nschema_prompt", span)?;
    let schema = schema_obj(args, 0, "nschema_prompt", span)?;
    let title = if args.len() > 1 {
        string_arg(args, 1, "nschema_prompt", span)?
    } else {
        "Return structured JSON.".into()
    };
    Ok(Value::String(schema_to_prompt(&schema, &title, span)?).ref_cell())
}

fn nschema_tool(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nschema_tool", span)?;
    let name = string_arg(args, 0, "nschema_tool", span)?;
    if name.is_empty() {
        return Ok(nschema_err(span, "nschema_tool() name must not be empty"));
    }
    let description = string_arg(args, 1, "nschema_tool", span)?;
    let schema = schema_obj(args, 2, "nschema_tool", span)?;
    let mut out = HashMap::new();
    out.insert("name".into(), Value::String(name).ref_cell());
    out.insert("description".into(), Value::String(description).ref_cell());
    out.insert(
        "parameters".into(),
        Value::Object(schema).ref_cell(),
    );
    Ok(Value::Object(out).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nschema_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nschema_fns![
    ("nschema_from_example", "from_example", nschema_from_example),
    ("nschema_validate", "validate", nschema_validate),
    ("nschema_coerce", "coerce", nschema_coerce),
    ("nschema_parse", "parse", nschema_parse),
    ("nschema_prompt", "prompt", nschema_prompt),
    ("nschema_tool", "tool", nschema_tool),
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

pub const MODULE_NAME: &str = "nschema";
pub const MODULE_PATHS: &[&str] = &["nschema", "std/nschema"];

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
    fn infer_object_schema() {
        let mut obj = HashMap::new();
        obj.insert("name".into(), Value::String("Niao".into()).ref_cell());
        obj.insert("age".into(), Value::Int(3).ref_cell());
        let schema = nschema_from_example(&[Value::Object(obj).ref_cell()], span()).unwrap();
        let schema_b = schema.borrow();
        match &*schema_b {
            Value::Object(s) => {
                assert_eq!(schema_type(s).as_deref(), Some("object"));
                let props = schema_properties(s).unwrap();
                assert!(props.contains_key("name"));
                assert!(props.contains_key("age"));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn validate_and_coerce() {
        let mut schema = HashMap::new();
        schema.insert("type".into(), Value::String("object".into()).ref_cell());
        let mut props = HashMap::new();
        let mut age_rule = HashMap::new();
        age_rule.insert("type".into(), Value::String("integer".into()).ref_cell());
        props.insert("age".into(), Value::Object(age_rule).ref_cell());
        schema.insert("properties".into(), Value::Object(props).ref_cell());
        schema.insert(
            "required".into(),
            Value::Array(vec![Value::String("age".into()).ref_cell()]).ref_cell(),
        );

        let mut val = HashMap::new();
        val.insert("age".into(), Value::String("42".into()).ref_cell());
        let coerced = nschema_coerce(
            &[Value::Object(val).ref_cell(), Value::Object(schema.clone()).ref_cell()],
            span(),
        )
        .unwrap();
        let coerced_b = coerced.borrow();
        match &*coerced_b {
            Value::Object(m) => match &*m.get("age").unwrap().borrow() {
                Value::Int(42) => {}
                other => panic!("expected int 42, got {other:?}"),
            },
            other => panic!("expected object, got {other:?}"),
        }

        let check = nschema_validate(
            &[Rc::clone(&coerced), Value::Object(schema).ref_cell()],
            span(),
        )
        .unwrap();
        let check_b = check.borrow();
        match &*check_b {
            Value::Object(r) => assert!(matches!(&*r.get("ok").unwrap().borrow(), Value::Bool(true))),
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn tool_spec_shape() {
        let mut schema = HashMap::new();
        schema.insert("type".into(), Value::String("object".into()).ref_cell());
        let tool = nschema_tool(
            &[
                Value::String("search".into()).ref_cell(),
                Value::String("Search the web".into()).ref_cell(),
                Value::Object(schema).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let tool_b = tool.borrow();
        match &*tool_b {
            Value::Object(m) => {
                assert_eq!(
                    match &*m.get("name").unwrap().borrow() {
                        Value::String(s) => s.as_str(),
                        _ => "",
                    },
                    "search"
                );
                assert!(m.contains_key("parameters"));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
