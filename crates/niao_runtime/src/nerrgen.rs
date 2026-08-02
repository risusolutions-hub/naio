//! Native nerrgen standard library — parse E-code spec files and generate
//! Rust / Niao / Markdown artifacts.
//!
//! Import with `import "nerrgen"` (or `import "std/nerrgen"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::rc::Rc;

const E3240_NERRGEN_ARITY: u32 = 3240;
const E3241_NERRGEN_ERROR: u32 = 3241;
const E3242_NERRGEN_TYPE: u32 = 3242;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3240_NERRGEN_ARITY,
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
            E3240_NERRGEN_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3242_NERRGEN_TYPE, msg.into())
}

fn gen_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3241_NERRGEN_ERROR, "nerrgen_error", msg.into(), span)
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

// ---------------------------------------------------------------------------
// Spec model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct ErrEntry {
    code: u32,
    name: String,
    message: String,
    kind: String,
    line: usize,
}

fn parse_spec_line(line: &str, line_no: usize) -> Option<Result<ErrEntry, String>> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    // E2900 name | message | kind
    // 2900 name message kind
    let parts: Vec<&str> = if trimmed.contains('|') {
        trimmed.split('|').map(|p| p.trim()).collect()
    } else {
        trimmed.split_whitespace().collect()
    };
    if parts.len() < 3 {
        return Some(Err(format!(
            "line {line_no}: expected at least code, name, and message"
        )));
    }
    let code_str = parts[0].trim_start_matches('E').trim_start_matches('e');
    let code: u32 = match code_str.parse() {
        Ok(c) => c,
        Err(_) => return Some(Err(format!("line {line_no}: invalid code '{code_str}'"))),
    };
    let name = parts[1].to_string();
    let message = parts[2].to_string();
    let kind = if parts.len() >= 4 {
        parts[3].to_string()
    } else {
        infer_kind(&name)
    };
    Some(Ok(ErrEntry {
        code,
        name,
        message,
        kind,
        line: line_no,
    }))
}

fn infer_kind(name: &str) -> String {
    if let Some(idx) = name.find('_') {
        format!("{}_error", &name[..idx])
    } else {
        "error".into()
    }
}

fn parse_spec(spec: &str) -> Result<Vec<ErrEntry>, String> {
    let mut entries = Vec::new();
    for (idx, line) in spec.lines().enumerate() {
        if let Some(res) = parse_spec_line(line, idx + 1) {
            entries.push(res?);
        }
    }
    if entries.is_empty() {
        return Err("spec contains no error entries".into());
    }
    entries.sort_by_key(|e| e.code);
    Ok(entries)
}

fn entry_obj(e: &ErrEntry) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("code".to_string(), Value::Int(e.code as i64).ref_cell());
    m.insert("name".to_string(), Value::String(e.name.clone()).ref_cell());
    m.insert(
        "message".to_string(),
        Value::String(e.message.clone()).ref_cell(),
    );
    m.insert("kind".to_string(), Value::String(e.kind.clone()).ref_cell());
    m.insert("line".to_string(), Value::Int(e.line as i64).ref_cell());
    Value::Object(m).ref_cell()
}

fn rust_const_name(entry: &ErrEntry) -> String {
    format!("E{}_{}", entry.code, entry.name.to_uppercase())
}

fn gen_rust(entries: &[ErrEntry]) -> String {
    let mut out = String::from("//! Generated error codes — do not edit by hand.\n\n");
    for e in entries {
        out.push_str(&format!("/// {} — {}\n", e.message, e.kind));
        out.push_str(&format!(
            "pub const {}: u32 = {};\n\n",
            rust_const_name(e),
            e.code
        ));
    }
    out.push_str("/// Kind names for generated codes.\n");
    out.push_str("pub fn generated_kind(code: u32) -> Option<&'static str> {\n");
    out.push_str("    match code {\n");
    for e in entries {
        out.push_str(&format!("        {} => Some(\"{}\"),\n", e.code, e.kind));
    }
    out.push_str("        _ => None,\n");
    out.push_str("    }\n");
    out.push_str("}\n");
    out
}

fn gen_niao(entries: &[ErrEntry]) -> String {
    let mut out = String::from("// Generated error table\nlet errors = [\n");
    for e in entries {
        let msg = e.message.replace('"', "\\\"");
        out.push_str(&format!(
            "    {{code: {}, name: \"{}\", message: \"{}\", kind: \"{}\"}},\n",
            e.code, e.name, msg, e.kind
        ));
    }
    out.push_str("]\n");
    out
}

fn gen_markdown(entries: &[ErrEntry], title: &str) -> String {
    let mut out = format!("# {title}\n\n");
    out.push_str("| Code | Name | Message | Kind |\n");
    out.push_str("|------|------|---------|------|\n");
    for e in entries {
        let msg = e.message.replace('|', "\\|");
        out.push_str(&format!(
            "| {} | `{}` | {} | `{}` |\n",
            e.code, e.name, msg, e.kind
        ));
    }
    out.push_str("\n## Errors\n\n");
    for e in entries {
        out.push_str(&format!("| {} | {} |\n", e.code, e.message));
    }
    out
}

fn parse_entries_arg(args: &[ValueRef], span: Span) -> NiaoResult<Vec<ErrEntry>> {
    // Accept raw spec string or parsed array from nerrgen_parse.
    if args.is_empty() {
        return Err(type_err(span, "missing spec"));
    }
    match &*args[0].borrow() {
        Value::String(spec) => parse_spec(spec).map_err(|m| type_err(span, m)),
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                match &*item.borrow() {
                    Value::Object(m) => {
                        let code = m
                            .get("code")
                            .and_then(|v| match &*v.borrow() {
                                Value::Int(n) => Some(*n as u32),
                                _ => None,
                            })
                            .ok_or_else(|| type_err(span, "entry missing code"))?;
                        let name = m
                            .get("name")
                            .and_then(|v| match &*v.borrow() {
                                Value::String(s) => Some(s.clone()),
                                _ => None,
                            })
                            .ok_or_else(|| type_err(span, "entry missing name"))?;
                        let message = m
                            .get("message")
                            .and_then(|v| match &*v.borrow() {
                                Value::String(s) => Some(s.clone()),
                                _ => None,
                            })
                            .ok_or_else(|| type_err(span, "entry missing message"))?;
                        let kind = m
                            .get("kind")
                            .and_then(|v| match &*v.borrow() {
                                Value::String(s) => Some(s.clone()),
                                _ => None,
                            })
                            .unwrap_or_else(|| infer_kind(&name));
                        let line = m
                            .get("line")
                            .and_then(|v| match &*v.borrow() {
                                Value::Int(n) => Some(*n as usize),
                                _ => None,
                            })
                            .unwrap_or(0);
                        out.push(ErrEntry {
                            code,
                            name,
                            message,
                            kind,
                            line,
                        });
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!("each entry must be an object, got {}", other.type_name()),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "spec must be a string or parsed entry array, got {}",
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nerrgen_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nerrgen_parse", span)?;
    let spec = string_arg(args, 0, "nerrgen_parse", span)?;
    match parse_spec(&spec) {
        Ok(entries) => Ok(Value::Array(entries.iter().map(entry_obj).collect()).ref_cell()),
        Err(msg) => Ok(gen_err(span, msg)),
    }
}

fn nerrgen_gen(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nerrgen_gen", span)?;
    let entries = parse_entries_arg(args, span)?;
    let format = string_arg(args, 1, "nerrgen_gen", span)?;
    let text = match format.as_str() {
        "rust" => gen_rust(&entries),
        "niao" => gen_niao(&entries),
        "markdown" | "md" => gen_markdown(&entries, "Error codes"),
        other => {
            return Ok(gen_err(
                span,
                format!("unknown format '{other}' (use rust, niao, markdown)"),
            ))
        }
    };
    Ok(Value::String(text).ref_cell())
}

fn nerrgen_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nerrgen_all", span)?;
    let entries = parse_entries_arg(args, span)?;
    let title = if args.len() == 2 {
        string_arg(args, 1, "nerrgen_all", span)?
    } else {
        "Error codes".into()
    };
    let mut m = HashMap::new();
    m.insert(
        "rust".to_string(),
        Value::String(gen_rust(&entries)).ref_cell(),
    );
    m.insert(
        "niao".to_string(),
        Value::String(gen_niao(&entries)).ref_cell(),
    );
    m.insert(
        "markdown".to_string(),
        Value::String(gen_markdown(&entries, &title)).ref_cell(),
    );
    Ok(Value::Object(m).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nerrgen_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nerrgen_fns![
    ("nerrgen_parse", "parse", nerrgen_parse),
    ("nerrgen_gen", "gen", nerrgen_gen),
    ("nerrgen_all", "all", nerrgen_all),
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

pub const MODULE_NAME: &str = "nerrgen";
pub const MODULE_PATHS: &[&str] = &["nerrgen", "std/nerrgen"];

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

    #[test]
    fn parse_spec_lines() {
        let spec = r#"
# demo module
E2900 nsemver_arity | Wrong argument count | nsemver_error
E2901 nsemver_error | Semantic error | nsemver_error
"#;
        let v = nerrgen_parse(&[s(spec)], span()).unwrap();
        let v_ref = v.borrow();
        match &*v_ref {
            Value::Array(items) => assert_eq!(items.len(), 2),
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn gen_rust_contains_const() {
        let spec = "2900 foo_arity | bad arity | foo_error\n";
        let parsed = nerrgen_parse(&[s(spec)], span()).unwrap();
        let rust = nerrgen_gen(&[parsed, s("rust")], span()).unwrap();
        let rust_ref = rust.borrow();
        match &*rust_ref {
            Value::String(text) => {
                assert!(text.contains("pub const E2900_FOO_ARITY"));
                assert!(text.contains("2900"));
            }
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn all_formats() {
        let spec = "3210 ndoc_arity | arity | ndoc_error\n";
        let parsed = nerrgen_parse(&[s(spec)], span()).unwrap();
        let all = nerrgen_all(&[parsed], span()).unwrap();
        let all_ref = all.borrow();
        match &*all_ref {
            Value::Object(m) => {
                assert!(m.contains_key("rust"));
                assert!(m.contains_key("niao"));
                assert!(m.contains_key("markdown"));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
