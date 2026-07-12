//! Native nconfig standard library — layered configuration:
//! defaults → file (json/toml) → env → args, with typed schema validation.
//!
//! Import with `import "nconfig"` (or `import "std/nconfig"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_bignum::BigInt;
use niao_json_core::toml::parse_to_value;
use niao_json_core::{parse as parse_json, Number as JNumber, Value as JsonValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E3160_NCONFIG_ARITY: u32 = 3160;
const E3161_NCONFIG_ERROR: u32 = 3161;
const E3162_NCONFIG_TYPE: u32 = 3162;
const E3163_NCONFIG_MISSING: u32 = 3163;

// ---------------------------------------------------------------------------
// Config store
// ---------------------------------------------------------------------------

struct Config {
    values: HashMap<String, ValueRef>,
    schema: Option<HashMap<String, ValueRef>>,
}

thread_local! {
    static CONFIGS: RefCell<HashMap<i64, Config>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

#[inline]
fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn with_config<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Config) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    CONFIGS.with(|configs| {
        let mut configs = configs.borrow_mut();
        match configs.get_mut(&id) {
            Some(cfg) => Ok(Ok(f(cfg))),
            None => Ok(Err(error_value(
                E3161_NCONFIG_ERROR,
                "nconfig_error",
                format!("invalid or closed config handle {id}"),
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
            E3160_NCONFIG_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3160_NCONFIG_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3162_NCONFIG_TYPE, msg.into())
}

fn config_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3161_NCONFIG_ERROR, "nconfig_error", msg.into(), span)
}

fn missing_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3163_NCONFIG_MISSING, "nconfig_error", msg.into(), span)
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

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn object_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn argv_list(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<String>> {
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
                                "{name}() expects argv array of strings, got {} at index {i}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// JsonValue ↔ Niao Value
// ---------------------------------------------------------------------------

fn json_to_value(j: JsonValue) -> Value {
    match j {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(b) => Value::Bool(b),
        JsonValue::Number(n) => match n {
            JNumber::I64(i) => Value::Int(i),
            JNumber::U64(u) if u <= i64::MAX as u64 => Value::Int(u as i64),
            JNumber::U64(u) => Value::BigInt(BigInt::from(u)),
            JNumber::F64(f) => {
                if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                    Value::Int(f as i64)
                } else {
                    Value::Float(f)
                }
            }
        },
        JsonValue::String(s) => Value::String(s),
        JsonValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(json_to_value(item).ref_cell());
            }
            Value::Array(out)
        }
        JsonValue::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, v) in map.iter() {
                out.insert(k.to_string(), json_to_value(v.clone()).ref_cell());
            }
            Value::Object(out)
        }
    }
}

fn parse_json_text(text: &str, span: Span) -> NiaoResult<Value> {
    parse_json(text)
        .map(json_to_value)
        .map_err(|e| RuntimeError::at(span, E3161_NCONFIG_ERROR, format!("invalid JSON: {e}")))
}

fn parse_file_text(path: &str, text: &str, span: Span) -> NiaoResult<Value> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".toml") {
        parse_to_value(text)
            .map(json_to_value)
            .map_err(|e| RuntimeError::at(span, E3161_NCONFIG_ERROR, format!("invalid TOML: {e}")))
    } else if lower.ends_with(".json") || lower.ends_with(".jsonc") {
        parse_json_text(text, span)
    } else {
        parse_json_text(text, span).or_else(|_| {
            parse_to_value(text)
                .map(json_to_value)
                .map_err(|e| RuntimeError::at(span, E3161_NCONFIG_ERROR, format!("invalid config file: {e}")))
        })
    }
}

// ---------------------------------------------------------------------------
// Merge / path helpers
// ---------------------------------------------------------------------------

fn clone_object(map: &HashMap<String, ValueRef>) -> HashMap<String, ValueRef> {
    map.iter()
        .map(|(k, v)| (k.clone(), Rc::clone(v)))
        .collect()
}

fn deep_merge(base: &mut HashMap<String, ValueRef>, overlay: &HashMap<String, ValueRef>) {
    for (k, v) in overlay {
        let merged_child = match (base.get(k).map(|x| x.borrow().clone()), v.borrow().clone()) {
            (Some(Value::Object(mut b)), Value::Object(o)) => {
                deep_merge(&mut b, &o);
                Some(b)
            }
            _ => None,
        };
        if let Some(b) = merged_child {
            base.insert(k.clone(), Value::Object(b).ref_cell());
        } else {
            base.insert(k.clone(), Rc::clone(v));
        }
    }
}

fn set_path(root: &mut HashMap<String, ValueRef>, path: &str, value: ValueRef) {
    let parts: Vec<&str> = path.split('.').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return;
    }
    if parts.len() == 1 {
        root.insert(parts[0].to_string(), value);
        return;
    }
    let head = parts[0].to_string();
    let tail = parts[1..].join(".");
    let child = root
        .entry(head)
        .or_insert_with(|| Value::Object(HashMap::new()).ref_cell());
    if let Value::Object(ref mut map) = *child.borrow_mut() {
        set_path(map, &tail, value);
    }
}

fn get_path(root: &HashMap<String, ValueRef>, path: &str) -> Option<ValueRef> {
    let parts: Vec<&str> = path.split('.').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }
    let mut cur = Rc::clone(root.get(parts[0])?);
    for part in &parts[1..] {
        let next = {
            let borrowed = cur.borrow();
            match &*borrowed {
                Value::Object(map) => map.get(*part).map(Rc::clone),
                _ => None,
            }
        };
        cur = next?;
    }
    Some(cur)
}

fn coerce_env_value(raw: &str) -> ValueRef {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("true") {
        return Value::Bool(true).ref_cell();
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return Value::Bool(false).ref_cell();
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return Value::Int(n).ref_cell();
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        if f.is_finite() {
            return Value::Float(f).ref_cell();
        }
    }
    if (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with('{') && trimmed.ends_with('}'))
    {
        if let Ok(j) = parse_json(trimmed) {
            return json_to_value(j).ref_cell();
        }
    }
    Value::String(raw.to_string()).ref_cell()
}

fn env_key_to_path(prefix: &str, key: &str) -> String {
    let rest = if prefix.is_empty() {
        key.to_string()
    } else if let Some(stripped) = key.strip_prefix(prefix) {
        stripped.to_string()
    } else {
        return String::new();
    };
    rest.trim_start_matches('_')
        .to_ascii_lowercase()
        .replace('_', ".")
}

fn parse_args_layer(argv: &[String]) -> HashMap<String, ValueRef> {
    let mut out = HashMap::new();
    let mut positionals: Vec<ValueRef> = Vec::new();
    let mut i = 0usize;
    while i < argv.len() {
        let arg = &argv[i];
        if arg == "--" {
            positionals.extend(argv[i + 1..].iter().map(|s| Value::String(s.clone()).ref_cell()));
            break;
        }
        if let Some(rest) = arg.strip_prefix("--") {
            if rest.is_empty() {
                i += 1;
                continue;
            }
            if let Some((k, v)) = rest.split_once('=') {
                let path = k.trim_start_matches('-').to_ascii_lowercase().replace('-', ".");
                set_path(&mut out, &path, coerce_env_value(v));
                i += 1;
                continue;
            }
            let path = rest.to_ascii_lowercase().replace('-', ".");
            if i + 1 < argv.len() && !argv[i + 1].starts_with('-') {
                let v = &argv[i + 1];
                set_path(&mut out, &path, coerce_env_value(v));
                i += 2;
            } else {
                set_path(&mut out, &path, Value::Bool(true).ref_cell());
                i += 1;
            }
            continue;
        }
        if let Some(rest) = arg.strip_prefix('-') {
            if rest.len() == 1 {
                let path = rest.to_ascii_lowercase();
                set_path(&mut out, &path, Value::Bool(true).ref_cell());
            } else {
                for ch in rest.chars() {
                    let path = ch.to_string();
                    set_path(&mut out, &path, Value::Bool(true).ref_cell());
                }
            }
            i += 1;
            continue;
        }
        positionals.push(Value::String(arg.clone()).ref_cell());
        i += 1;
    }
    if !positionals.is_empty() {
        out.insert("_args".to_string(), Value::Array(positionals).ref_cell());
    }
    out
}

// ---------------------------------------------------------------------------
// Schema validation
// ---------------------------------------------------------------------------

fn obj_str(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn obj_bool(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        None => default,
        _ => default,
    }
}

fn value_type_name(v: &Value) -> &'static str {
    match v {
        Value::Int(_) => "int",
        Value::BigInt(_) => "bigint",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::Bool(_) => "bool",
        Value::Array(_) | Value::IntArray(_) | Value::FloatArray(_) | Value::BoolArray(_)
        | Value::ByteArray(_) | Value::StringArray(_) => "array",
        Value::Object(_) => "object",
        Value::Nil => "nil",
        _ => "unknown",
    }
}

fn type_matches(expected: &str, v: &Value) -> bool {
    match expected {
        "string" | "str" => matches!(v, Value::String(_)),
        "int" => matches!(v, Value::Int(_) | Value::BigInt(_)),
        "float" | "number" => {
            matches!(v, Value::Int(_) | Value::Float(_) | Value::BigInt(_))
        }
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
        "any" => true,
        _ => false,
    }
}

fn int_from_value(v: &Value) -> Option<i64> {
    match v {
        Value::Int(n) => Some(*n),
        Value::Float(f) if f.fract() == 0.0 && *f >= i64::MIN as f64 && *f <= i64::MAX as f64 => {
            Some(*f as i64)
        }
        _ => None,
    }
}

fn validate_field(
    path: &str,
    value: &Value,
    rule: &HashMap<String, ValueRef>,
    span: Span,
) -> Result<(), ValueRef> {
    let ty = obj_str(rule, "type").unwrap_or_else(|| "any".into());
    if !type_matches(&ty, value) {
        return Err(config_err(
            span,
            format!(
                "config field '{path}' expected type '{ty}', got {}",
                value_type_name(value)
            ),
        ));
    }
    if ty == "int" || ty == "float" || ty == "number" {
        if let Some(n) = int_from_value(value) {
            if let Some(min_v) = rule.get("min").and_then(|v| int_from_value(&v.borrow())) {
                if n < min_v {
                    return Err(config_err(
                        span,
                        format!("config field '{path}' value {n} < min {min_v}"),
                    ));
                }
            }
            if let Some(max_v) = rule.get("max").and_then(|v| int_from_value(&v.borrow())) {
                if n > max_v {
                    return Err(config_err(
                        span,
                        format!("config field '{path}' value {n} > max {max_v}"),
                    ));
                }
            }
        }
    }
    if ty == "array" {
        if let Some(items_rule) = rule.get("items") {
            if let Value::Object(items_spec) = &*items_rule.borrow() {
                if let Value::Array(items) = value {
                    for (i, item) in items.iter().enumerate() {
                        validate_field(&format!("{path}[{i}]"), &item.borrow(), items_spec, span)?;
                    }
                }
            }
        }
    }
    if ty == "object" {
        if let Some(props) = rule.get("properties") {
            if let Value::Object(props_map) = &*props.borrow() {
                if let Value::Object(val_map) = value {
                    for (k, prop_rule) in props_map {
                        if let Value::Object(rule_map) = &*prop_rule.borrow() {
                            let sub = format!("{path}.{k}");
                            match val_map.get(k) {
                                Some(v) => validate_field(&sub, &v.borrow(), rule_map, span)?,
                                None if obj_bool(rule_map, "required", false) => {
                                    return Err(missing_err(
                                        span,
                                        format!("config field '{sub}' is required"),
                                    ));
                                }
                                None => {}
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn apply_defaults(
    values: &mut HashMap<String, ValueRef>,
    schema: &HashMap<String, ValueRef>,
) {
    for (k, rule) in schema {
        let Value::Object(rule_map) = &*rule.borrow() else {
            continue;
        };
        if values.contains_key(k) {
            if let Some(props) = rule_map.get("properties") {
                let val_clone = values.get(k).map(|v| v.borrow().clone());
                let props_clone = props.borrow().clone();
                if let (Some(Value::Object(val_map)), Value::Object(props_map)) =
                    (val_clone, props_clone)
                {
                    let mut nested = clone_object(&val_map);
                    let nested_schema: HashMap<String, ValueRef> = props_map
                        .iter()
                        .map(|(pk, pr)| (pk.clone(), Rc::clone(pr)))
                        .collect();
                    apply_defaults(&mut nested, &nested_schema);
                    values.insert(k.clone(), Value::Object(nested).ref_cell());
                }
            }
            continue;
        }
        if let Some(def) = rule_map.get("default") {
            values.insert(k.clone(), Rc::clone(def));
        } else if let Some(props) = rule_map.get("properties") {
            if let Value::Object(props_map) = &*props.borrow() {
                let mut nested = HashMap::new();
                let nested_schema: HashMap<String, ValueRef> = props_map
                    .iter()
                    .map(|(pk, pr)| (pk.clone(), Rc::clone(pr)))
                    .collect();
                apply_defaults(&mut nested, &nested_schema);
                if !nested.is_empty() {
                    values.insert(k.clone(), Value::Object(nested).ref_cell());
                }
            }
        }
    }
}

fn validate_config(cfg: &mut Config, span: Span) -> Result<(), ValueRef> {
    let Some(schema) = cfg.schema.clone() else {
        return Ok(());
    };
    apply_defaults(&mut cfg.values, &schema);
    for (k, rule) in &schema {
        let Value::Object(rule_map) = &*rule.borrow() else {
            continue;
        };
        let required = obj_bool(rule_map, "required", false);
        match cfg.values.get(k) {
            Some(v) => validate_field(k, &v.borrow(), rule_map, span)?,
            None if required => {
                return Err(missing_err(
                    span,
                    format!("config field '{k}' is required"),
                ));
            }
            None => {}
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nconfig_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nconfig_new", span)?;
    let mut values = HashMap::new();
    if !args.is_empty() {
        let obj = object_arg(args, 0, "nconfig_new", span)?;
        deep_merge(&mut values, &obj);
    }
    let id = new_handle();
    CONFIGS.with(|configs| {
        configs.borrow_mut().insert(
            id,
            Config {
                values,
                schema: None,
            },
        );
    });
    Ok(Value::Int(id).ref_cell())
}

fn nconfig_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nconfig_file", span)?;
    let id = int_arg(args, 0, "nconfig_file", span)?;
    let path = string_arg(args, 1, "nconfig_file", span)?;
    let text = fs::read_to_string(&path).map_err(|e| {
        RuntimeError::at(
            span,
            E3161_NCONFIG_ERROR,
            format!("nconfig_file: cannot read '{path}': {e}"),
        )
    })?;
    let parsed = parse_file_text(&path, &text, span)?;
    let layer = match parsed {
        Value::Object(map) => map,
        other => {
            let mut map = HashMap::new();
            map.insert("value".to_string(), other.ref_cell());
            map
        }
    };
    match with_config(id, span, |cfg| deep_merge(&mut cfg.values, &layer))? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nconfig_env(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nconfig_env", span)?;
    let id = int_arg(args, 0, "nconfig_env", span)?;
    let prefix = if args.len() > 1 {
        string_arg(args, 1, "nconfig_env", span)?
    } else {
        String::new()
    };
    let mut layer = HashMap::new();
    for (k, v) in env::vars() {
        let path = env_key_to_path(&prefix, &k);
        if path.is_empty() {
            continue;
        }
        set_path(&mut layer, &path, coerce_env_value(&v));
    }
    match with_config(id, span, |cfg| deep_merge(&mut cfg.values, &layer))? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nconfig_args(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nconfig_args", span)?;
    let id = int_arg(args, 0, "nconfig_args", span)?;
    let argv = if args.len() > 1 {
        argv_list(args, 1, "nconfig_args", span)?
    } else {
        env::args().skip(1).collect()
    };
    let layer = parse_args_layer(&argv);
    match with_config(id, span, |cfg| deep_merge(&mut cfg.values, &layer))? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nconfig_schema(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nconfig_schema", span)?;
    let id = int_arg(args, 0, "nconfig_schema", span)?;
    let schema = object_arg(args, 1, "nconfig_schema", span)?;
    match with_config(id, span, |cfg| cfg.schema = Some(schema))? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nconfig_validate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nconfig_validate", span)?;
    let id = int_arg(args, 0, "nconfig_validate", span)?;
    match with_config(id, span, |cfg| validate_config(cfg, span))? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nconfig_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nconfig_get", span)?;
    let id = int_arg(args, 0, "nconfig_get", span)?;
    let key = if args.len() > 1 {
        Some(string_arg(args, 1, "nconfig_get", span)?)
    } else {
        None
    };
    match with_config(id, span, |cfg| {
        if let Some(k) = key {
            match get_path(&cfg.values, &k) {
                Some(v) => Ok(v),
                None => Err(missing_err(
                    span,
                    format!("config key '{k}' not found"),
                )),
            }
        } else {
            Ok(Value::Object(clone_object(&cfg.values)).ref_cell())
        }
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nconfig_resolve(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nconfig_resolve", span)?;
    let id = int_arg(args, 0, "nconfig_resolve", span)?;
    match with_config(id, span, |cfg| {
        let mut copy = cfg.clone_values();
        if let Some(schema) = cfg.schema.clone() {
            apply_defaults(&mut copy, &schema);
        }
        Value::Object(copy).ref_cell()
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn nconfig_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nconfig_close", span)?;
    let id = int_arg(args, 0, "nconfig_close", span)?;
    let removed = CONFIGS.with(|configs| configs.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

impl Config {
    fn clone_values(&self) -> HashMap<String, ValueRef> {
        clone_object(&self.values)
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nconfig_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nconfig_fns![
    ("nconfig_new", "new", nconfig_new),
    ("nconfig_file", "file", nconfig_file),
    ("nconfig_env", "env", nconfig_env),
    ("nconfig_args", "args", nconfig_args),
    ("nconfig_schema", "schema", nconfig_schema),
    ("nconfig_validate", "validate", nconfig_validate),
    ("nconfig_get", "get", nconfig_get),
    ("nconfig_resolve", "resolve", nconfig_resolve),
    ("nconfig_close", "close", nconfig_close),
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

pub const MODULE_NAME: &str = "nconfig";
pub const MODULE_PATHS: &[&str] = &["nconfig", "std/nconfig"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn span() -> Span {
        Span::dummy()
    }

    fn obj(pairs: &[(&str, Value)]) -> ValueRef {
        let mut map = HashMap::new();
        for (k, v) in pairs {
            map.insert((*k).to_string(), v.clone().ref_cell());
        }
        Value::Object(map).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> i64 {
        match &*r.unwrap().borrow() {
            Value::Int(n) => *n,
            other => panic!("expected handle, got {other:?}"),
        }
    }

    #[test]
    fn layered_defaults_file_env_args() {
        let h = handle(nconfig_new(
            &[obj(&[("port", Value::Int(8080)), ("host", Value::String("localhost".into()))])],
            span(),
        ));

        let path = std::env::temp_dir().join("nconfig_test_layer.json");
        fs::write(&path, r#"{"port": 3000, "debug": true}"#).unwrap();
        let path = path.to_string_lossy().to_string();
        nconfig_file(&[Value::Int(h).ref_cell(), Value::String(path).ref_cell()], span()).unwrap();

        std::env::set_var("APP_HOST", "prod.example");
        nconfig_env(
            &[Value::Int(h).ref_cell(), Value::String("APP_".into()).ref_cell()],
            span(),
        )
        .unwrap();
        std::env::remove_var("APP_HOST");

        nconfig_args(
            &[
                Value::Int(h).ref_cell(),
                Value::Array(vec![
                    Value::String("--verbose".into()).ref_cell(),
                    Value::String("--port=9000".into()).ref_cell(),
                ])
                .ref_cell(),
            ],
            span(),
        )
        .unwrap();

        let all = nconfig_get(&[Value::Int(h).ref_cell()], span()).unwrap();
        match &*all.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map["port"].borrow(), Value::Int(9000)));
                assert!(matches!(
                    &*map["host"].borrow(),
                    Value::String(s) if s == "prod.example"
                ));
                assert!(matches!(&*map["debug"].borrow(), Value::Bool(true)));
                assert!(matches!(&*map["verbose"].borrow(), Value::Bool(true)));
            }
            other => panic!("expected object, got {other:?}"),
        }
        nconfig_close(&[Value::Int(h).ref_cell()], span()).unwrap();
    }

    #[test]
    fn schema_validate_and_defaults() {
        let h = handle(nconfig_new(&[obj(&[("port", Value::Int(42))])], span()));
        let schema = obj(&[
            (
                "port",
                Value::Object({
                    let mut m = HashMap::new();
                    m.insert("type".to_string(), Value::String("int".into()).ref_cell());
                    m.insert("min".to_string(), Value::Int(1).ref_cell());
                    m.insert("max".to_string(), Value::Int(100).ref_cell());
                    m.insert("required".to_string(), Value::Bool(true).ref_cell());
                    m
                }),
            ),
            (
                "debug",
                Value::Object({
                    let mut m = HashMap::new();
                    m.insert("type".to_string(), Value::String("bool".into()).ref_cell());
                    m.insert("default".to_string(), Value::Bool(false).ref_cell());
                    m
                }),
            ),
        ]);
        nconfig_schema(
            &[Value::Int(h).ref_cell(), schema],
            span(),
        )
        .unwrap();
        let ok = nconfig_validate(&[Value::Int(h).ref_cell()], span()).unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(true)));

        let resolved = nconfig_resolve(&[Value::Int(h).ref_cell()], span()).unwrap();
        match &*resolved.borrow() {
            Value::Object(map) => {
                assert!(map.contains_key("debug"));
                assert!(matches!(&*map["debug"].borrow(), Value::Bool(false)));
            }
            other => panic!("expected object, got {other:?}"),
        }
        nconfig_close(&[Value::Int(h).ref_cell()], span()).unwrap();
    }

    #[test]
    fn missing_key_error() {
        let h = handle(nconfig_new(&[], span()));
        let v = nconfig_get(
            &[Value::Int(h).ref_cell(), Value::String("nope".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
        nconfig_close(&[Value::Int(h).ref_cell()], span()).unwrap();
    }

    #[test]
    fn parse_args_layer_flags() {
        let layer = parse_args_layer(&[
            "--verbose".into(),
            "-x".into(),
            "--port".into(),
            "42".into(),
            "pos".into(),
        ]);
        assert!(matches!(
            &*layer["verbose"].borrow(),
            Value::Bool(true)
        ));
        assert!(matches!(&*layer["x"].borrow(), Value::Bool(true)));
        assert!(matches!(&*layer["port"].borrow(), Value::Int(42)));
        assert!(matches!(&*layer["_args"].borrow(), Value::Array(a) if a.len() == 1));
    }
}
