//! Native nguard standard library — PII scan/redact (email, phone, SSN,
//! credit card with Luhn, IP, API keys) and denylist middleware hooks.
//!
//! Import with `import "nguard"` (or `import "std/nguard"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, StringArray, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E3320_NGUARD_ARITY: u32 = 3320;
const E3321_NGUARD_ERROR: u32 = 3321;
const E3322_NGUARD_TYPE: u32 = 3322;

const ALL_TYPES: &[&str] = &["email", "phone", "ssn", "card", "ip", "api_key"];

// ---------------------------------------------------------------------------
// Denylist registry
// ---------------------------------------------------------------------------

thread_local! {
    static DENYLIST: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3320_NGUARD_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3320_NGUARD_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3322_NGUARD_TYPE, msg.into())
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

fn optional_object_arg(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Some(map.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn optional_string_array_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Option<Vec<String>>> {
    if args.len() <= idx {
        return Ok(None);
    }
    match &*args[idx].borrow() {
        Value::Nil => Ok(None),
        Value::StringArray(items) => Ok(Some(items.dense_vec())),
        Value::Array(items) => {
            let mut out = Vec::new();
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects string array at argument {}, element {} is {}",
                                idx + 1,
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(Some(out))
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn string_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: &str) -> String {
    let Some(map) = map else {
        return default.to_string();
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) if !s.is_empty() => s,
        _ => default.to_string(),
    }
}

fn bool_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        Some(Value::String(s)) => matches!(s.as_str(), "true" | "1" | "yes" | "on"),
        _ => default,
    }
}

fn nguard_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3321_NGUARD_ERROR, "nguard_error", msg.into(), span)
}

// ---------------------------------------------------------------------------
// PII detection
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct Finding {
    kind: String,
    start: usize,
    end: usize,
    matched: String,
}

fn luhn_valid(digits: &str) -> bool {
    let nums: Vec<u32> = digits.chars().filter_map(|c| c.to_digit(10)).collect();
    if nums.len() < 13 || nums.len() > 19 {
        return false;
    }
    let mut sum = 0u32;
    let mut alt = false;
    for &d in nums.iter().rev() {
        let mut n = d;
        if alt {
            n *= 2;
            if n > 9 {
                n -= 9;
            }
        }
        sum += n;
        alt = !alt;
    }
    sum % 10 == 0
}

fn is_email(s: &str) -> bool {
    let Some(at) = s.find('@') else {
        return false;
    };
    if at == 0 || at + 1 >= s.len() {
        return false;
    }
    let local = &s[..at];
    let domain = &s[at + 1..];
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || ".@_+-".contains(c))
}

fn scan_emails(text: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for word in text.split_whitespace() {
        let trimmed: String = word
            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '@' && c != '.' && c != '_' && c != '+' && c != '-')
            .to_string();
        if is_email(&trimmed) {
            if let Some(pos) = text.find(&trimmed) {
                out.push(Finding {
                    kind: "email".into(),
                    start: pos,
                    end: pos + trimmed.len(),
                    matched: trimmed,
                });
            }
        }
    }
    out
}

fn scan_phones(text: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() || bytes[i] == b'+' || bytes[i] == b'(' {
            let start = i;
            let mut digits = 0usize;
            while i < bytes.len() {
                let b = bytes[i];
                if b.is_ascii_digit() {
                    digits += 1;
                    i += 1;
                } else if matches!(b, b' ' | b'-' | b'.' | b'(' | b')' | b'+') {
                    i += 1;
                } else {
                    break;
                }
            }
            if (10..=15).contains(&digits) {
                let matched = text[start..i].to_string();
                out.push(Finding {
                    kind: "phone".into(),
                    start,
                    end: i,
                    matched,
                });
            }
        } else {
            i += 1;
        }
    }
    out
}

fn scan_ssn(text: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 10 < bytes.len() {
        if bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3] == b'-'
            && bytes[i + 4].is_ascii_digit()
            && bytes[i + 5].is_ascii_digit()
            && bytes[i + 6] == b'-'
            && bytes[i + 7].is_ascii_digit()
            && bytes[i + 8].is_ascii_digit()
            && bytes[i + 9].is_ascii_digit()
            && bytes[i + 10].is_ascii_digit()
        {
            let matched = &text[i..i + 11];
            out.push(Finding {
                kind: "ssn".into(),
                start: i,
                end: i + 11,
                matched: matched.to_string(),
            });
            i += 11;
        } else {
            i += 1;
        }
    }
    out
}

fn scan_cards(text: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            let mut digits = String::new();
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b' ' || bytes[i] == b'-') {
                if bytes[i].is_ascii_digit() {
                    digits.push(bytes[i] as char);
                }
                i += 1;
            }
            if (13..=19).contains(&digits.len()) && luhn_valid(&digits) {
                out.push(Finding {
                    kind: "card".into(),
                    start,
                    end: i,
                    matched: text[start..i].to_string(),
                });
            }
        } else {
            i += 1;
        }
    }
    out
}

fn is_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| p.parse::<u8>().is_ok())
}

fn is_ipv6(s: &str) -> bool {
    if !s.contains(':') {
        return false;
    }
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 3 || parts.len() > 8 {
        return false;
    }
    parts.iter().all(|p| p.is_empty() || p.chars().all(|c| c.is_ascii_hexdigit()))
}

fn scan_ips(text: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| !c.is_ascii_hexdigit() && c != '.' && c != ':');
        if is_ipv4(trimmed) || is_ipv6(trimmed) {
            if let Some(pos) = text.find(trimmed) {
                out.push(Finding {
                    kind: "ip".into(),
                    start: pos,
                    end: pos + trimmed.len(),
                    matched: trimmed.to_string(),
                });
            }
        }
    }
    out
}

fn scan_api_keys(text: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    let prefixes = ["sk-", "sk_live_", "sk_test_", "api_key=", "apikey=", "Bearer ", "AKIA"];
    for prefix in prefixes {
        let mut start = 0;
        while let Some(pos) = text[start..].find(prefix) {
            let abs = start + pos;
            let rest_start = abs + prefix.len();
            let mut end = rest_start;
            let bytes = text.as_bytes();
            while end < bytes.len() {
                let b = bytes[end];
                if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.') {
                    end += 1;
                } else {
                    break;
                }
            }
            if end > rest_start + 7 {
                out.push(Finding {
                    kind: "api_key".into(),
                    start: abs,
                    end,
                    matched: text[abs..end].to_string(),
                });
            }
            start = abs + prefix.len();
        }
    }
    out
}

fn scan_text(text: &str, types: &[&str]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for kind in types {
        let batch = match *kind {
            "email" => scan_emails(text),
            "phone" => scan_phones(text),
            "ssn" => scan_ssn(text),
            "card" => scan_cards(text),
            "ip" => scan_ips(text),
            "api_key" => scan_api_keys(text),
            _ => Vec::new(),
        };
        findings.extend(batch);
    }
    findings.sort_by_key(|f| f.start);
    findings.dedup_by(|a, b| a.start == b.start && a.end == b.end && a.kind == b.kind);
    findings
}

fn redact_label(kind: &str, replacement: &str) -> String {
    if replacement == "[REDACTED]" {
        format!("[{kind}]")
    } else {
        replacement.to_string()
    }
}

fn redact_text(text: &str, findings: &[Finding], replacement: &str) -> String {
    if findings.is_empty() {
        return text.to_string();
    }
    let mut out = String::new();
    let mut cursor = 0;
    for f in findings {
        if f.start > cursor {
            out.push_str(&text[cursor..f.start]);
        }
        out.push_str(&redact_label(&f.kind, replacement));
        cursor = f.end;
    }
    if cursor < text.len() {
        out.push_str(&text[cursor..]);
    }
    out
}

fn finding_object(f: &Finding) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("type".to_string(), Value::String(f.kind.clone()).ref_cell());
    map.insert("start".to_string(), Value::Int(f.start as i64).ref_cell());
    map.insert("end".to_string(), Value::Int(f.end as i64).ref_cell());
    map.insert("match".to_string(), Value::String(f.matched.clone()).ref_cell());
    Value::Object(map).ref_cell()
}

fn parse_types_arg(
    types: Option<Vec<String>>,
    span: Span,
    name: &str,
) -> Result<Vec<&'static str>, RuntimeError> {
    let Some(types) = types else {
        return Ok(ALL_TYPES.to_vec());
    };
    let mut out = Vec::new();
    for t in types {
        if let Some(k) = ALL_TYPES.iter().copied().find(|&x| x == t.as_str()) {
            out.push(k);
        } else {
            return Err(type_err(
                span,
                format!(
                    "{name}() unknown PII type '{t}', expected one of: {}",
                    ALL_TYPES.join(", ")
                ),
            ));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nguard_scan(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nguard_scan", span)?;
    let text = string_arg(args, 0, "nguard_scan", span)?;
    let types = parse_types_arg(optional_string_array_arg(args, 1, "nguard_scan", span)?, span, "nguard_scan")?;
    let findings = scan_text(&text, &types);
    let items: Vec<ValueRef> = findings.iter().map(finding_object).collect();
    let mut map = HashMap::new();
    map.insert("count".to_string(), Value::Int(findings.len() as i64).ref_cell());
    map.insert("findings".to_string(), Value::Array(items).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

fn nguard_redact(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nguard_redact", span)?;
    let text = string_arg(args, 0, "nguard_redact", span)?;
    let opts = optional_object_arg(args, 1);
    let replacement = string_field(opts.as_ref(), "replacement", "[REDACTED]");
    let types_list = opts.as_ref().and_then(|m| {
        m.get("types").and_then(|v| match &*v.borrow() {
            Value::StringArray(sa) => Some(sa.dense_vec()),
            _ => None,
        })
    });
    let types = parse_types_arg(types_list, span, "nguard_redact")?;
    let findings = scan_text(&text, &types);
    Ok(Value::String(redact_text(&text, &findings, &replacement)).ref_cell())
}

fn nguard_has_pii(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nguard_has_pii", span)?;
    let text = string_arg(args, 0, "nguard_has_pii", span)?;
    let types = parse_types_arg(optional_string_array_arg(args, 1, "nguard_has_pii", span)?, span, "nguard_has_pii")?;
    Ok(Value::Bool(!scan_text(&text, &types).is_empty()).ref_cell())
}

fn nguard_denylist_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nguard_denylist_add", span)?;
    let pattern = string_arg(args, 0, "nguard_denylist_add", span)?;
    if pattern.is_empty() {
        return Ok(nguard_err(span, "nguard_denylist_add() pattern must not be empty"));
    }
    DENYLIST.with(|list| {
        let mut list = list.borrow_mut();
        if !list.iter().any(|p| p == &pattern) {
            list.push(pattern);
        }
    });
    Ok(Value::Nil.ref_cell())
}

fn nguard_denylist_remove(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nguard_denylist_remove", span)?;
    let pattern = string_arg(args, 0, "nguard_denylist_remove", span)?;
    let removed = DENYLIST.with(|list| {
        let mut list = list.borrow_mut();
        let before = list.len();
        list.retain(|p| p != &pattern);
        before != list.len()
    });
    Ok(Value::Bool(removed).ref_cell())
}

fn nguard_denylist_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nguard_denylist_clear", span)?;
    DENYLIST.with(|list| list.borrow_mut().clear());
    Ok(Value::Nil.ref_cell())
}

fn nguard_denylist_check(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nguard_denylist_check", span)?;
    let text = string_arg(args, 0, "nguard_denylist_check", span)?;
    let lower = text.to_ascii_lowercase();
    let matches: Vec<String> = DENYLIST.with(|list| {
        list.borrow()
            .iter()
            .filter(|p| lower.contains(&p.to_ascii_lowercase()))
            .cloned()
            .collect()
    });
    let mut map = HashMap::new();
    map.insert("blocked".to_string(), Value::Bool(!matches.is_empty()).ref_cell());
    map.insert("matches".to_string(), Value::StringArray(StringArray::dense(matches)).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

fn nguard_filter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nguard_filter", span)?;
    let text = string_arg(args, 0, "nguard_filter", span)?;
    let opts = optional_object_arg(args, 1);
    let redact = bool_field(opts.as_ref(), "redact", true);
    let block = bool_field(opts.as_ref(), "block", true);
    let replacement = string_field(opts.as_ref(), "replacement", "[REDACTED]");
    let working = if redact {
        let findings = scan_text(&text, ALL_TYPES);
        redact_text(&text, &findings, &replacement)
    } else {
        text.clone()
    };
    let check = nguard_denylist_check(&[Value::String(working.clone()).ref_cell()], span)?;
    let blocked = match &*check.borrow() {
        Value::Object(map) => matches!(&*map.get("blocked").unwrap().borrow(), Value::Bool(true)),
        _ => false,
    };
    if block && blocked {
        return Ok(nguard_err(span, "text blocked by denylist"));
    }
    Ok(Value::String(working).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nguard_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nguard_fns![
    ("nguard_scan", "scan", nguard_scan),
    ("nguard_redact", "redact", nguard_redact),
    ("nguard_has_pii", "has_pii", nguard_has_pii),
    ("nguard_denylist_add", "denylist_add", nguard_denylist_add),
    ("nguard_denylist_remove", "denylist_remove", nguard_denylist_remove),
    ("nguard_denylist_clear", "denylist_clear", nguard_denylist_clear),
    ("nguard_denylist_check", "denylist_check", nguard_denylist_check),
    ("nguard_filter", "filter", nguard_filter),
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

pub const MODULE_NAME: &str = "nguard";
pub const MODULE_PATHS: &[&str] = &["nguard", "std/nguard"];

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
    fn detects_email_and_redacts() {
        let text = "Contact alice@example.com please";
        let findings = scan_text(text, ALL_TYPES);
        assert!(findings.iter().any(|f| f.kind == "email"));
        assert_eq!(
            redact_text(text, &findings, "[REDACTED]"),
            "Contact [email] please"
        );
    }

    #[test]
    fn luhn_and_ssn() {
        assert!(luhn_valid("4111111111111111"));
        assert!(!luhn_valid("4111111111111112"));
        let text = "ssn 123-45-6789 here";
        let findings = scan_ssn(text);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].matched, "123-45-6789");
    }

    #[test]
    fn denylist_blocks() {
        DENYLIST.with(|l| l.borrow_mut().clear());
        nguard_denylist_add(&[s("forbidden")], span()).unwrap();
        let out = nguard_filter(&[s("this is forbidden text")], span()).unwrap();
        assert!(matches!(&*out.borrow(), Value::Error(_)));
    }
}
