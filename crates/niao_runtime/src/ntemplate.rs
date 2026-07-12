//! Native ntemplate standard library — versioned prompt templates with
//! `{{var}}` injection and token-count estimation for context budgeting.
//!
//! Import with `import "ntemplate"` (or `import "std/ntemplate"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, StringArray, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E3300_NTEMPLATE_ARITY: u32 = 3300;
const E3301_NTEMPLATE_ERROR: u32 = 3301;
const E3302_NTEMPLATE_TYPE: u32 = 3302;

// ---------------------------------------------------------------------------
// Template registry
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct TemplateEntry {
    version: String,
    body: String,
}

thread_local! {
    static TEMPLATES: RefCell<HashMap<String, Vec<TemplateEntry>>> =
        RefCell::new(HashMap::new());
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3300_NTEMPLATE_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3300_NTEMPLATE_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3302_NTEMPLATE_TYPE, msg.into())
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

fn optional_string_arg(args: &[ValueRef], idx: usize) -> Option<String> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn object_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
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

fn ntemplate_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3301_NTEMPLATE_ERROR, "ntemplate_error", msg.into(), span)
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Nil => String::new(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Template parsing / rendering
// ---------------------------------------------------------------------------

fn extract_vars(template: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let bytes = template.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            i += 2;
            let start = i;
            while i + 1 < bytes.len() && !(bytes[i] == b'}' && bytes[i + 1] == b'}') {
                i += 1;
            }
            if i + 1 < bytes.len() {
                let name = template[start..i].trim();
                if !name.is_empty() && !vars.iter().any(|v| v == name) {
                    vars.push(name.to_string());
                }
                i += 2;
            } else {
                break;
            }
        } else {
            i += 1;
        }
    }
    vars
}

fn render_template(template: &str, vars: &HashMap<String, ValueRef>) -> Result<String, String> {
    let mut out = String::new();
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            i += 2;
            let start = i;
            while i + 1 < bytes.len() && !(bytes[i] == b'}' && bytes[i + 1] == b'}') {
                i += 1;
            }
            if i + 1 >= bytes.len() {
                return Err("unclosed template variable '{{'".into());
            }
            let name = template[start..i].trim();
            if name.is_empty() {
                return Err("empty template variable name".into());
            }
            let val = vars
                .get(name)
                .map(|v| value_to_string(&v.borrow()))
                .unwrap_or_default();
            out.push_str(&val);
            i += 2;
        } else {
            let ch = template[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    Ok(out)
}

/// Heuristic token estimate: ~4 chars per token for Latin text, min 1 for non-empty.
fn estimate_tokens(text: &str) -> i64 {
    if text.is_empty() {
        return 0;
    }
    let chars = text.chars().count() as f64;
    let words = text.split_whitespace().count() as f64;
    let by_chars = (chars / 4.0).ceil();
    let by_words = (words * 1.3).ceil();
    by_chars.max(by_words).max(1.0) as i64
}

fn version_key(v: &str) -> (u64, u64, u64, String) {
    let mut parts = v.splitn(3, '.');
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch, v.to_string())
}

fn sort_versions(mut versions: Vec<String>) -> Vec<String> {
    versions.sort_by(|a, b| version_key(a).cmp(&version_key(b)));
    versions
}

fn find_entry<'a>(entries: &'a [TemplateEntry], version: Option<&str>) -> Option<&'a TemplateEntry> {
    if entries.is_empty() {
        return None;
    }
    if let Some(ver) = version {
        entries.iter().find(|e| e.version == ver)
    } else {
        entries
            .iter()
            .max_by(|a, b| version_key(&a.version).cmp(&version_key(&b.version)))
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ntemplate_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ntemplate_set", span)?;
    let name = string_arg(args, 0, "ntemplate_set", span)?;
    let version = string_arg(args, 1, "ntemplate_set", span)?;
    let body = string_arg(args, 2, "ntemplate_set", span)?;
    if name.is_empty() {
        return Ok(ntemplate_err(span, "ntemplate_set() name must not be empty"));
    }
    if version.is_empty() {
        return Ok(ntemplate_err(span, "ntemplate_set() version must not be empty"));
    }
    TEMPLATES.with(|store| {
        let mut store = store.borrow_mut();
        let entries = store.entry(name).or_default();
        if let Some(e) = entries.iter_mut().find(|e| e.version == version) {
            e.body = body;
        } else {
            entries.push(TemplateEntry { version, body });
        }
    });
    Ok(Value::Nil.ref_cell())
}

fn ntemplate_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntemplate_get", span)?;
    let name = string_arg(args, 0, "ntemplate_get", span)?;
    let version = optional_string_arg(args, 1);
    let body = TEMPLATES.with(|store| {
        let store = store.borrow();
        store
            .get(&name)
            .and_then(|entries| find_entry(entries, version.as_deref()))
            .map(|e| e.body.clone())
    });
    match body {
        Some(b) => Ok(Value::String(b).ref_cell()),
        None => Ok(ntemplate_err(
            span,
            format!("template '{name}' not found{}", version.map(|v| format!(" at version '{v}'")).unwrap_or_default()),
        )),
    }
}

fn ntemplate_versions(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntemplate_versions", span)?;
    let name = string_arg(args, 0, "ntemplate_versions", span)?;
    let versions = TEMPLATES.with(|store| {
        let store = store.borrow();
        store
            .get(&name)
            .map(|entries| sort_versions(entries.iter().map(|e| e.version.clone()).collect()))
            .unwrap_or_default()
    });
    Ok(Value::StringArray(StringArray::dense(versions)).ref_cell())
}

fn ntemplate_vars(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntemplate_vars", span)?;
    let template = string_arg(args, 0, "ntemplate_vars", span)?;
    Ok(Value::StringArray(StringArray::dense(extract_vars(&template))).ref_cell())
}

fn ntemplate_render_str(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ntemplate_render_str", span)?;
    let template = string_arg(args, 0, "ntemplate_render_str", span)?;
    let vars = object_arg(args, 1, "ntemplate_render_str", span)?;
    match render_template(&template, &vars) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(msg) => Ok(ntemplate_err(span, msg)),
    }
}

fn ntemplate_render(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntemplate_render", span)?;
    let name = string_arg(args, 0, "ntemplate_render", span)?;
    let vars = object_arg(args, 1, "ntemplate_render", span)?;
    let version = optional_string_arg(args, 2);
    let body = TEMPLATES.with(|store| {
        let store = store.borrow();
        store
            .get(&name)
            .and_then(|entries| find_entry(entries, version.as_deref()))
            .map(|e| e.body.clone())
    });
    let Some(template) = body else {
        return Ok(ntemplate_err(
            span,
            format!("template '{name}' not found{}", version.map(|v| format!(" at version '{v}'")).unwrap_or_default()),
        ));
    };
    match render_template(&template, &vars) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(msg) => Ok(ntemplate_err(span, msg)),
    }
}

fn ntemplate_estimate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntemplate_estimate", span)?;
    let text = string_arg(args, 0, "ntemplate_estimate", span)?;
    Ok(Value::Int(estimate_tokens(&text)).ref_cell())
}

fn ntemplate_remove(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntemplate_remove", span)?;
    let name = string_arg(args, 0, "ntemplate_remove", span)?;
    let version = optional_string_arg(args, 1);
    let removed = TEMPLATES.with(|store| {
        let mut store = store.borrow_mut();
        match store.get_mut(&name) {
            None => false,
            Some(entries) => {
                if let Some(ver) = version {
                    let before = entries.len();
                    entries.retain(|e| e.version != ver);
                    let changed = entries.len() != before;
                    if entries.is_empty() {
                        store.remove(&name);
                    }
                    changed
                } else {
                    store.remove(&name).is_some()
                }
            }
        }
    });
    Ok(Value::Bool(removed).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ntemplate_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ntemplate_fns![
    ("ntemplate_set", "set", ntemplate_set),
    ("ntemplate_get", "get", ntemplate_get),
    ("ntemplate_versions", "versions", ntemplate_versions),
    ("ntemplate_vars", "vars", ntemplate_vars),
    ("ntemplate_render_str", "render_str", ntemplate_render_str),
    ("ntemplate_render", "render", ntemplate_render),
    ("ntemplate_estimate", "estimate", ntemplate_estimate),
    ("ntemplate_remove", "remove", ntemplate_remove),
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

pub const MODULE_NAME: &str = "ntemplate";
pub const MODULE_PATHS: &[&str] = &["ntemplate", "std/ntemplate"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn vars(map: &[(&str, &str)]) -> ValueRef {
        let mut m = HashMap::new();
        for (k, v) in map {
            m.insert(k.to_string(), s(v));
        }
        Value::Object(m).ref_cell()
    }

    #[test]
    fn render_and_estimate() {
        let out = render_template("Hi {{name}}, count={{n}}", &{
            let mut m = HashMap::new();
            m.insert("name".to_string(), s("Ada"));
            m.insert("n".to_string(), Value::Int(3).ref_cell());
            m
        })
        .unwrap();
        assert_eq!(out, "Hi Ada, count=3");
        assert!(estimate_tokens("hello world test") >= 2);
        assert_eq!(extract_vars("{{a}} and {{b}} and {{a}}"), vec!["a", "b"]);
    }

    #[test]
    fn versioned_registry() {
        TEMPLATES.with(|t| t.borrow_mut().clear());
        ntemplate_set(&[s("greet"), s("1.0.0"), s("Hello {{name}}")], span()).unwrap();
        ntemplate_set(&[s("greet"), s("2.0.0"), s("Hey {{name}}!")], span()).unwrap();
        let v = ntemplate_get(&[s("greet"), s("1.0.0")], span()).unwrap();
        assert_eq!(&*v.borrow().to_string(), "Hello {{name}}");
        let rendered = ntemplate_render(&[s("greet"), vars(&[("name", "Bob")])], span()).unwrap();
        assert_eq!(&*rendered.borrow().to_string(), "Hey Bob!");
        let versions_val = ntemplate_versions(&[s("greet")], span()).unwrap().borrow().clone();
        match versions_val {
            Value::StringArray(vs) => assert_eq!(vs.dense_vec(), vec!["1.0.0".to_string(), "2.0.0".to_string()]),
            other => panic!("expected string_array, got {other:?}"),
        }
    }
}
