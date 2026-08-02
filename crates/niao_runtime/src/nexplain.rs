//! Native nexplain standard library — actionable error enrichment:
//! match error messages against built-in and custom pattern hints,
//! return `{message, hint, fix, code?}` and pretty multi-line strings.
//!
//! Import with `import "nexplain"` (or `import "std/nexplain"`).

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E3010_NEXPLAIN_ARITY: u32 = 3010;
const E3011_NEXPLAIN_ERROR: u32 = 3011;
const E3012_NEXPLAIN_TYPE: u32 = 3012;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3010_NEXPLAIN_ARITY,
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
            E3010_NEXPLAIN_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3012_NEXPLAIN_TYPE, msg.into())
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
// Hint rules
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct HintRule {
    pattern: String,
    hint: String,
    fix: String,
}

fn builtin_rules() -> Vec<HintRule> {
    vec![
        HintRule {
            pattern: "undefined".into(),
            hint: "A name was used before it was defined".into(),
            fix: "Check spelling, or declare/import the name before use".into(),
        },
        HintRule {
            pattern: "arity".into(),
            hint: "Wrong number of arguments for a call".into(),
            fix: "Pass the expected number of arguments (see the function docs)".into(),
        },
        HintRule {
            pattern: "type".into(),
            hint: "A value had the wrong type".into(),
            fix: "Convert or pass a value of the expected type".into(),
        },
        HintRule {
            pattern: "import".into(),
            hint: "Module import failed".into(),
            fix: "Verify the module path/name and that the package is installed".into(),
        },
        HintRule {
            pattern: "division".into(),
            hint: "Invalid division (often division by zero)".into(),
            fix: "Ensure the divisor is non-zero before dividing".into(),
        },
        HintRule {
            pattern: "nil".into(),
            hint: "Unexpected nil value".into(),
            fix: "Check for nil before reading fields or calling methods".into(),
        },
        HintRule {
            pattern: "handle".into(),
            hint: "Invalid, closed, or unknown handle".into(),
            fix: "Use a valid open handle; do not use a handle after close".into(),
        },
        HintRule {
            pattern: "permission".into(),
            hint: "Permission or access was denied".into(),
            fix: "Check credentials, ACLs, file modes, or capability grants".into(),
        },
        HintRule {
            pattern: "denied".into(),
            hint: "Permission or access was denied".into(),
            fix: "Check credentials, ACLs, file modes, or capability grants".into(),
        },
        HintRule {
            pattern: "timeout".into(),
            hint: "The operation timed out".into(),
            fix: "Increase the timeout, retry with backoff, or check network health".into(),
        },
    ]
}

thread_local! {
    static CUSTOM_RULES: RefCell<Vec<HintRule>> = const { RefCell::new(Vec::new()) };
}

const DEFAULT_HINT: &str = "No specific hint matched this message";
const DEFAULT_FIX: &str = "Read the message carefully and inspect the surrounding code";

fn find_rule(message: &str) -> Option<HintRule> {
    let lower = message.to_ascii_lowercase();
    CUSTOM_RULES
        .with(|rules| {
            for rule in rules.borrow().iter() {
                if lower.contains(&rule.pattern.to_ascii_lowercase()) {
                    return Some(rule.clone());
                }
            }
            None
        })
        .or_else(|| {
            for rule in builtin_rules() {
                if lower.contains(&rule.pattern.to_ascii_lowercase()) {
                    return Some(rule);
                }
            }
            None
        })
}

struct Explained {
    message: String,
    hint: String,
    fix: String,
    code: Option<u32>,
}

fn explain_message(message: String, code: Option<u32>) -> Explained {
    let rule = find_rule(&message);
    Explained {
        hint: rule
            .as_ref()
            .map(|r| r.hint.clone())
            .unwrap_or_else(|| DEFAULT_HINT.into()),
        fix: rule
            .as_ref()
            .map(|r| r.fix.clone())
            .unwrap_or_else(|| DEFAULT_FIX.into()),
        message,
        code,
    }
}

fn extract_msg_or_error(
    arg: &ValueRef,
    name: &str,
    span: Span,
) -> NiaoResult<(String, Option<u32>)> {
    match &*arg.borrow() {
        Value::String(s) => Ok((s.clone(), None)),
        Value::Error(e) => Ok((e.message.clone(), Some(e.code))),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string or error, got {}",
                other.type_name()
            ),
        )),
    }
}

fn explained_to_object(e: Explained) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("message".into(), Value::String(e.message).ref_cell());
    map.insert("hint".into(), Value::String(e.hint).ref_cell());
    map.insert("fix".into(), Value::String(e.fix).ref_cell());
    if let Some(code) = e.code {
        map.insert("code".into(), Value::Int(code as i64).ref_cell());
    }
    Value::Object(map).ref_cell()
}

fn format_explained(e: &Explained) -> String {
    let mut out = String::new();
    out.push_str("Message: ");
    out.push_str(&e.message);
    out.push('\n');
    out.push_str("Hint:    ");
    out.push_str(&e.hint);
    out.push('\n');
    out.push_str("Fix:     ");
    out.push_str(&e.fix);
    if let Some(code) = e.code {
        out.push('\n');
        out.push_str(&format!("Code:    E{code:04}"));
    }
    out
}

fn rule_to_object(rule: &HintRule) -> ValueRef {
    let mut map = HashMap::new();
    map.insert(
        "pattern".into(),
        Value::String(rule.pattern.clone()).ref_cell(),
    );
    map.insert("hint".into(), Value::String(rule.hint.clone()).ref_cell());
    map.insert("fix".into(), Value::String(rule.fix.clone()).ref_cell());
    Value::Object(map).ref_cell()
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nexplain_of(msg_or_error) → {message, hint, fix, code?}
fn nexplain_of(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nexplain_of", span)?;
    let (message, code) = extract_msg_or_error(&args[0], "nexplain_of", span)?;
    Ok(explained_to_object(explain_message(message, code)))
}

/// nexplain_register(pattern, hint, fix?) → true
fn nexplain_register(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nexplain_register", span)?;
    let pattern = string_arg(args, 0, "nexplain_register", span)?;
    let hint = string_arg(args, 1, "nexplain_register", span)?;
    let fix = if args.len() >= 3 {
        string_arg(args, 2, "nexplain_register", span)?
    } else {
        String::new()
    };
    if pattern.is_empty() {
        return Err(RuntimeError::at(
            span,
            E3011_NEXPLAIN_ERROR,
            "nexplain_register() pattern must not be empty",
        ));
    }
    CUSTOM_RULES.with(|rules| {
        rules.borrow_mut().push(HintRule { pattern, hint, fix });
    });
    Ok(Value::Bool(true).ref_cell())
}

/// nexplain_hints() → [{pattern, hint, fix}, ...]
fn nexplain_hints(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nexplain_hints", span)?;
    let mut out: Vec<ValueRef> = Vec::new();
    CUSTOM_RULES.with(|rules| {
        for rule in rules.borrow().iter() {
            out.push(rule_to_object(rule));
        }
    });
    for rule in builtin_rules() {
        out.push(rule_to_object(&rule));
    }
    Ok(Value::Array(out).ref_cell())
}

/// nexplain_format(msg_or_error) → pretty multi-line string
fn nexplain_format(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nexplain_format", span)?;
    let (message, code) = extract_msg_or_error(&args[0], "nexplain_format", span)?;
    let explained = explain_message(message, code);
    Ok(Value::String(format_explained(&explained)).ref_cell())
}

/// nexplain_clear_custom() → true (builtins retained)
fn nexplain_clear_custom(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nexplain_clear_custom", span)?;
    CUSTOM_RULES.with(|rules| rules.borrow_mut().clear());
    Ok(Value::Bool(true).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nexplain_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nexplain_fns![
    ("nexplain_of", "of", nexplain_of),
    ("nexplain_register", "register", nexplain_register),
    ("nexplain_hints", "hints", nexplain_hints),
    ("nexplain_format", "format", nexplain_format),
    (
        "nexplain_clear_custom",
        "clear_custom",
        nexplain_clear_custom
    ),
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

pub const MODULE_NAME: &str = "nexplain";
pub const MODULE_PATHS: &[&str] = &["nexplain", "std/nexplain"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error_value;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    fn reset() {
        CUSTOM_RULES.with(|r| r.borrow_mut().clear());
    }

    #[test]
    fn of_matches_builtin_undefined() {
        reset();
        let msg = Value::String("undefined variable 'x'".into()).ref_cell();
        let r = nexplain_of(&[msg], span()).unwrap();
        match &*r.borrow() {
            Value::Object(map) => {
                assert!(
                    matches!(&*map.get("message").unwrap().borrow(), Value::String(s) if s.contains("undefined"))
                );
                assert!(
                    matches!(&*map.get("hint").unwrap().borrow(), Value::String(s) if s.contains("defined"))
                );
                assert!(
                    matches!(&*map.get("fix").unwrap().borrow(), Value::String(s) if !s.is_empty())
                );
                assert!(!map.contains_key("code"));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn of_extracts_error_code() {
        reset();
        let err = error_value(1503, "test_error", "invalid handle 9", span());
        let r = nexplain_of(&[err], span()).unwrap();
        match &*r.borrow() {
            Value::Object(map) => {
                assert!(matches!(
                    &*map.get("code").unwrap().borrow(),
                    Value::Int(1503)
                ));
                assert!(
                    matches!(&*map.get("hint").unwrap().borrow(), Value::String(s) if s.contains("handle"))
                );
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn custom_rules_override_builtins() {
        reset();
        nexplain_register(
            &[
                Value::String("timeout".into()).ref_cell(),
                Value::String("custom timeout hint".into()).ref_cell(),
                Value::String("custom fix".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let r = nexplain_of(
            &[Value::String("operation timeout".into()).ref_cell()],
            span(),
        )
        .unwrap();
        match &*r.borrow() {
            Value::Object(map) => {
                assert!(
                    matches!(&*map.get("hint").unwrap().borrow(), Value::String(s) if s == "custom timeout hint")
                );
                assert!(
                    matches!(&*map.get("fix").unwrap().borrow(), Value::String(s) if s == "custom fix")
                );
            }
            other => panic!("expected object, got {other:?}"),
        }
        nexplain_clear_custom(&[], span()).unwrap();
        let r2 = nexplain_of(
            &[Value::String("operation timeout".into()).ref_cell()],
            span(),
        )
        .unwrap();
        match &*r2.borrow() {
            Value::Object(map) => {
                assert!(
                    matches!(&*map.get("hint").unwrap().borrow(), Value::String(s) if s.contains("timed out"))
                );
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn hints_lists_custom_then_builtins() {
        reset();
        nexplain_register(
            &[
                Value::String("boom".into()).ref_cell(),
                Value::String("exploded".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let r = nexplain_hints(&[], span()).unwrap();
        match &*r.borrow() {
            Value::Array(items) => {
                assert!(items.len() >= 11); // 1 custom + 10 builtins
                match &*items[0].borrow() {
                    Value::Object(map) => {
                        assert!(
                            matches!(&*map.get("pattern").unwrap().borrow(), Value::String(s) if s == "boom")
                        );
                    }
                    other => panic!("expected object, got {other:?}"),
                }
            }
            other => panic!("expected array, got {other:?}"),
        }
        reset();
    }

    #[test]
    fn format_is_multiline() {
        reset();
        let s = nexplain_format(
            &[Value::String("nil value received".into()).ref_cell()],
            span(),
        )
        .unwrap();
        match &*s.borrow() {
            Value::String(text) => {
                assert!(text.contains("Message:"));
                assert!(text.contains("Hint:"));
                assert!(text.contains("Fix:"));
                assert!(text.contains('\n'));
            }
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn permission_and_denied_match() {
        reset();
        for msg in ["permission denied reading file", "access denied by ACL"] {
            let r = nexplain_of(&[Value::String(msg.into()).ref_cell()], span()).unwrap();
            match &*r.borrow() {
                Value::Object(map) => {
                    assert!(
                        matches!(&*map.get("hint").unwrap().borrow(), Value::String(s) if s.contains("denied"))
                    );
                }
                other => panic!("expected object, got {other:?}"),
            }
        }
    }

    #[test]
    fn unmatched_uses_defaults() {
        reset();
        let r = nexplain_of(
            &[Value::String("completely unknown xyzzy".into()).ref_cell()],
            span(),
        )
        .unwrap();
        match &*r.borrow() {
            Value::Object(map) => {
                assert!(
                    matches!(&*map.get("hint").unwrap().borrow(), Value::String(s) if s == DEFAULT_HINT)
                );
                assert!(
                    matches!(&*map.get("fix").unwrap().borrow(), Value::String(s) if s == DEFAULT_FIX)
                );
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn arity_and_type_errors() {
        reset();
        assert!(nexplain_of(&[], span()).is_err());
        assert!(nexplain_of(&[Value::Int(1).ref_cell()], span()).is_err());
        assert!(nexplain_register(&[Value::String("a".into()).ref_cell()], span()).is_err());
        assert!(nexplain_register(
            &[
                Value::String("".into()).ref_cell(),
                Value::String("h".into()).ref_cell(),
            ],
            span()
        )
        .is_err());
    }
}
