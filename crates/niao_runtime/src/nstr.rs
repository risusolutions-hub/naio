//! Native nstr standard library — fast, Unicode-correct string utilities:
//! case conversions, trimming/padding, search, split/join, wrapping,
//! slugify, and edit-distance helpers. Std-only, zero dependencies.
//!
//! Import with `import "nstr"` (or `import "std/nstr"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;

/// Refuse to build strings larger than this (light-RAM guard).
const MAX_BUILD_BYTES: usize = 64 * 1024 * 1024;

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
            codes::E2600_NSTR_ARITY,
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
            codes::E2600_NSTR_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
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

fn optional_string(args: &[ValueRef], idx: usize) -> Option<String> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn optional_int(args: &[ValueRef], idx: usize, default: i64) -> i64 {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Int(n) => *n,
        _ => default,
    }
}

fn nstr_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2601_NSTR_ERROR, "nstr_error", msg.into(), span)
}

fn bounds_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2603_NSTR_BOUNDS, "nstr_error", msg.into(), span)
}

fn str_val(s: String) -> NiaoResult<ValueRef> {
    Ok(Value::String(s).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

/// Extract a list of strings from a Value::Array or Value::StringArray.
fn string_list_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Vec<String>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects an array of strings, found {}",
                                other.type_name()
                            ),
                        ))
                    }
                }
            }
            Ok(out)
        }
        Value::StringArray(sa) => Ok(sa.dense_vec()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an array of strings as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Case conversion core
// ---------------------------------------------------------------------------

/// Split an identifier-ish string into lowercase word tokens.
/// Handles spaces, `-`, `_`, `.` separators and camelCase / HTTPServer boundaries.
fn split_words(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for i in 0..chars.len() {
        let c = chars[i];
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if !cur.is_empty() && c.is_uppercase() {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).map(|n| n.is_lowercase()).unwrap_or(false);
            // boundary: aB | 9B | ABc (acronym followed by normal word)
            if prev.is_lowercase() || prev.is_numeric() || (prev.is_uppercase() && next_lower) {
                words.push(std::mem::take(&mut cur));
            }
        }
        for lc in c.to_lowercase() {
            cur.push(lc);
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

fn capitalize_word(w: &str) -> String {
    let mut cs = w.chars();
    match cs.next() {
        Some(first) => first.to_uppercase().collect::<String>() + cs.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Builtin functions
// ---------------------------------------------------------------------------

fn nstr_upper(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_upper", span)?;
    str_val(string_arg(args, 0, "nstr_upper", span)?.to_uppercase())
}

fn nstr_lower(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_lower", span)?;
    str_val(string_arg(args, 0, "nstr_lower", span)?.to_lowercase())
}

fn nstr_title(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_title", span)?;
    let s = string_arg(args, 0, "nstr_title", span)?;
    let mut out = String::with_capacity(s.len());
    let mut at_boundary = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if at_boundary {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            at_boundary = false;
        } else {
            out.push(c);
            at_boundary = true;
        }
    }
    str_val(out)
}

fn nstr_capitalize(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_capitalize", span)?;
    let s = string_arg(args, 0, "nstr_capitalize", span)?;
    let lower = s.to_lowercase();
    str_val(capitalize_word(&lower))
}

fn nstr_swap_case(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_swap_case", span)?;
    let s = string_arg(args, 0, "nstr_swap_case", span)?;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_uppercase() {
            out.extend(c.to_lowercase());
        } else if c.is_lowercase() {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    str_val(out)
}

fn nstr_snake(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_snake", span)?;
    let s = string_arg(args, 0, "nstr_snake", span)?;
    str_val(split_words(&s).join("_"))
}

fn nstr_kebab(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_kebab", span)?;
    let s = string_arg(args, 0, "nstr_kebab", span)?;
    str_val(split_words(&s).join("-"))
}

fn nstr_camel(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_camel", span)?;
    let s = string_arg(args, 0, "nstr_camel", span)?;
    let words = split_words(&s);
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        if i == 0 {
            out.push_str(w);
        } else {
            out.push_str(&capitalize_word(w));
        }
    }
    str_val(out)
}

fn nstr_pascal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_pascal", span)?;
    let s = string_arg(args, 0, "nstr_pascal", span)?;
    let out: String = split_words(&s).iter().map(|w| capitalize_word(w)).collect();
    str_val(out)
}

fn nstr_constant(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_constant", span)?;
    let s = string_arg(args, 0, "nstr_constant", span)?;
    str_val(split_words(&s).join("_").to_uppercase())
}

fn nstr_trim(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_trim", span)?;
    str_val(string_arg(args, 0, "nstr_trim", span)?.trim().to_string())
}

fn nstr_trim_start(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_trim_start", span)?;
    str_val(
        string_arg(args, 0, "nstr_trim_start", span)?
            .trim_start()
            .to_string(),
    )
}

fn nstr_trim_end(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_trim_end", span)?;
    str_val(
        string_arg(args, 0, "nstr_trim_end", span)?
            .trim_end()
            .to_string(),
    )
}

fn nstr_trim_chars(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_trim_chars", span)?;
    let s = string_arg(args, 0, "nstr_trim_chars", span)?;
    let set = string_arg(args, 1, "nstr_trim_chars", span)?;
    let set: Vec<char> = set.chars().collect();
    str_val(s.trim_matches(|c| set.contains(&c)).to_string())
}

fn pad_impl(args: &[ValueRef], name: &str, span: Span, at_start: bool) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, name, span)?;
    let s = string_arg(args, 0, name, span)?;
    let width = int_arg(args, 1, name, span)?;
    let fill = optional_string(args, 2).unwrap_or_else(|| " ".to_string());
    let fill_char = fill.chars().next().unwrap_or(' ');
    if width < 0 {
        return Err(type_err(span, format!("{name}() width must be >= 0")));
    }
    let width = width as usize;
    if width > MAX_BUILD_BYTES {
        return Ok(nstr_err(span, format!("{name}() width too large")));
    }
    let len = s.chars().count();
    if len >= width {
        return str_val(s);
    }
    let pad: String = std::iter::repeat(fill_char).take(width - len).collect();
    if at_start {
        str_val(pad + &s)
    } else {
        str_val(s + &pad)
    }
}

fn nstr_pad_start(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    pad_impl(args, "nstr_pad_start", span, true)
}

fn nstr_pad_end(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    pad_impl(args, "nstr_pad_end", span, false)
}

fn nstr_center(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nstr_center", span)?;
    let s = string_arg(args, 0, "nstr_center", span)?;
    let width = int_arg(args, 1, "nstr_center", span)?;
    let fill = optional_string(args, 2).unwrap_or_else(|| " ".to_string());
    let fill_char = fill.chars().next().unwrap_or(' ');
    if width < 0 {
        return Err(type_err(
            span,
            "nstr_center() width must be >= 0".to_string(),
        ));
    }
    let width = width as usize;
    if width > MAX_BUILD_BYTES {
        return Ok(nstr_err(span, "nstr_center() width too large"));
    }
    let len = s.chars().count();
    if len >= width {
        return str_val(s);
    }
    let total = width - len;
    let left = total / 2;
    let right = total - left;
    let l: String = std::iter::repeat(fill_char).take(left).collect();
    let r: String = std::iter::repeat(fill_char).take(right).collect();
    str_val(l + &s + &r)
}

fn nstr_repeat(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_repeat", span)?;
    let s = string_arg(args, 0, "nstr_repeat", span)?;
    let n = int_arg(args, 1, "nstr_repeat", span)?;
    if n < 0 {
        return Err(type_err(
            span,
            "nstr_repeat() count must be >= 0".to_string(),
        ));
    }
    let n = n as usize;
    if s.len().saturating_mul(n) > MAX_BUILD_BYTES {
        return Ok(nstr_err(span, "nstr_repeat() result too large"));
    }
    str_val(s.repeat(n))
}

fn nstr_reverse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_reverse", span)?;
    let s = string_arg(args, 0, "nstr_reverse", span)?;
    str_val(s.chars().rev().collect())
}

fn nstr_truncate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nstr_truncate", span)?;
    let s = string_arg(args, 0, "nstr_truncate", span)?;
    let max = int_arg(args, 1, "nstr_truncate", span)?;
    let suffix = optional_string(args, 2).unwrap_or_else(|| "...".to_string());
    if max < 0 {
        return Err(type_err(
            span,
            "nstr_truncate() max must be >= 0".to_string(),
        ));
    }
    let max = max as usize;
    let len = s.chars().count();
    if len <= max {
        return str_val(s);
    }
    let suffix_len = suffix.chars().count();
    let keep = max.saturating_sub(suffix_len);
    let head: String = s.chars().take(keep).collect();
    str_val(head + &suffix)
}

fn nstr_split(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_split", span)?;
    let s = string_arg(args, 0, "nstr_split", span)?;
    let sep = string_arg(args, 1, "nstr_split", span)?;
    let out: Vec<ValueRef> = if sep.is_empty() {
        s.chars()
            .map(|c| Value::String(c.to_string()).ref_cell())
            .collect()
    } else {
        s.split(sep.as_str())
            .map(|p| Value::String(p.to_string()).ref_cell())
            .collect()
    };
    Ok(Value::Array(out).ref_cell())
}

fn nstr_split_n(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nstr_split_n", span)?;
    let s = string_arg(args, 0, "nstr_split_n", span)?;
    let sep = string_arg(args, 1, "nstr_split_n", span)?;
    let n = int_arg(args, 2, "nstr_split_n", span)?;
    if n <= 0 || sep.is_empty() {
        return Err(type_err(
            span,
            "nstr_split_n() expects n >= 1 and a non-empty separator".to_string(),
        ));
    }
    let out: Vec<ValueRef> = s
        .splitn(n as usize, sep.as_str())
        .map(|p| Value::String(p.to_string()).ref_cell())
        .collect();
    Ok(Value::Array(out).ref_cell())
}

fn nstr_split_ws(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_split_ws", span)?;
    let s = string_arg(args, 0, "nstr_split_ws", span)?;
    let out: Vec<ValueRef> = s
        .split_whitespace()
        .map(|p| Value::String(p.to_string()).ref_cell())
        .collect();
    Ok(Value::Array(out).ref_cell())
}

fn nstr_join(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_join", span)?;
    let items = string_list_arg(args, 0, "nstr_join", span)?;
    let sep = string_arg(args, 1, "nstr_join", span)?;
    str_val(items.join(&sep))
}

fn nstr_lines(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_lines", span)?;
    let s = string_arg(args, 0, "nstr_lines", span)?;
    let out: Vec<ValueRef> = s
        .lines()
        .map(|l| Value::String(l.to_string()).ref_cell())
        .collect();
    Ok(Value::Array(out).ref_cell())
}

fn nstr_contains(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_contains", span)?;
    let s = string_arg(args, 0, "nstr_contains", span)?;
    let needle = string_arg(args, 1, "nstr_contains", span)?;
    bool_val(s.contains(needle.as_str()))
}

fn nstr_starts_with(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_starts_with", span)?;
    let s = string_arg(args, 0, "nstr_starts_with", span)?;
    let p = string_arg(args, 1, "nstr_starts_with", span)?;
    bool_val(s.starts_with(p.as_str()))
}

fn nstr_ends_with(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_ends_with", span)?;
    let s = string_arg(args, 0, "nstr_ends_with", span)?;
    let p = string_arg(args, 1, "nstr_ends_with", span)?;
    bool_val(s.ends_with(p.as_str()))
}

/// Byte offset → char index for reporting positions consistently in chars.
fn char_index_of(s: &str, byte_pos: usize) -> i64 {
    s[..byte_pos].chars().count() as i64
}

fn nstr_index_of(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_index_of", span)?;
    let s = string_arg(args, 0, "nstr_index_of", span)?;
    let needle = string_arg(args, 1, "nstr_index_of", span)?;
    match s.find(needle.as_str()) {
        Some(pos) => int_val(char_index_of(&s, pos)),
        None => int_val(-1),
    }
}

fn nstr_last_index_of(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_last_index_of", span)?;
    let s = string_arg(args, 0, "nstr_last_index_of", span)?;
    let needle = string_arg(args, 1, "nstr_last_index_of", span)?;
    match s.rfind(needle.as_str()) {
        Some(pos) => int_val(char_index_of(&s, pos)),
        None => int_val(-1),
    }
}

fn nstr_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_count", span)?;
    let s = string_arg(args, 0, "nstr_count", span)?;
    let needle = string_arg(args, 1, "nstr_count", span)?;
    if needle.is_empty() {
        return int_val(0);
    }
    int_val(s.matches(needle.as_str()).count() as i64)
}

fn nstr_replace(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nstr_replace", span)?;
    let s = string_arg(args, 0, "nstr_replace", span)?;
    let from = string_arg(args, 1, "nstr_replace", span)?;
    let to = string_arg(args, 2, "nstr_replace", span)?;
    if from.is_empty() {
        return str_val(s);
    }
    str_val(s.replace(from.as_str(), &to))
}

fn nstr_replace_n(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "nstr_replace_n", span)?;
    let s = string_arg(args, 0, "nstr_replace_n", span)?;
    let from = string_arg(args, 1, "nstr_replace_n", span)?;
    let to = string_arg(args, 2, "nstr_replace_n", span)?;
    let n = int_arg(args, 3, "nstr_replace_n", span)?;
    if from.is_empty() || n <= 0 {
        return str_val(s);
    }
    str_val(s.replacen(from.as_str(), &to, n as usize))
}

fn nstr_remove_prefix(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_remove_prefix", span)?;
    let s = string_arg(args, 0, "nstr_remove_prefix", span)?;
    let p = string_arg(args, 1, "nstr_remove_prefix", span)?;
    str_val(s.strip_prefix(p.as_str()).unwrap_or(&s).to_string())
}

fn nstr_remove_suffix(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_remove_suffix", span)?;
    let s = string_arg(args, 0, "nstr_remove_suffix", span)?;
    let p = string_arg(args, 1, "nstr_remove_suffix", span)?;
    str_val(s.strip_suffix(p.as_str()).unwrap_or(&s).to_string())
}

fn nstr_substring(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nstr_substring", span)?;
    let s = string_arg(args, 0, "nstr_substring", span)?;
    let len = s.chars().count() as i64;
    let mut start = int_arg(args, 1, "nstr_substring", span)?;
    let mut end = optional_int(args, 2, len);
    // negative indices count from the end
    if start < 0 {
        start += len;
    }
    if end < 0 {
        end += len;
    }
    let start = start.clamp(0, len) as usize;
    let end = end.clamp(0, len) as usize;
    if start >= end {
        return str_val(String::new());
    }
    str_val(s.chars().skip(start).take(end - start).collect())
}

fn nstr_char_at(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_char_at", span)?;
    let s = string_arg(args, 0, "nstr_char_at", span)?;
    let mut idx = int_arg(args, 1, "nstr_char_at", span)?;
    let len = s.chars().count() as i64;
    if idx < 0 {
        idx += len;
    }
    if idx < 0 || idx >= len {
        return Ok(bounds_err(
            span,
            format!("nstr_char_at() index {idx} out of bounds for length {len}"),
        ));
    }
    match s.chars().nth(idx as usize) {
        Some(c) => str_val(c.to_string()),
        None => Ok(bounds_err(span, "nstr_char_at() index out of bounds")),
    }
}

fn nstr_chars(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_chars", span)?;
    let s = string_arg(args, 0, "nstr_chars", span)?;
    let out: Vec<ValueRef> = s
        .chars()
        .map(|c| Value::String(c.to_string()).ref_cell())
        .collect();
    Ok(Value::Array(out).ref_cell())
}

fn nstr_char_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_char_len", span)?;
    let s = string_arg(args, 0, "nstr_char_len", span)?;
    int_val(s.chars().count() as i64)
}

fn nstr_byte_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_byte_len", span)?;
    let s = string_arg(args, 0, "nstr_byte_len", span)?;
    int_val(s.len() as i64)
}

fn nstr_ord(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_ord", span)?;
    let s = string_arg(args, 0, "nstr_ord", span)?;
    match s.chars().next() {
        Some(c) => int_val(c as i64),
        None => Ok(nstr_err(span, "nstr_ord() on empty string")),
    }
}

fn nstr_chr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_chr", span)?;
    let n = int_arg(args, 0, "nstr_chr", span)?;
    let cp = u32::try_from(n).ok().and_then(char::from_u32);
    match cp {
        Some(c) => str_val(c.to_string()),
        None => Ok(nstr_err(span, format!("nstr_chr() invalid code point {n}"))),
    }
}

fn nstr_wrap(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_wrap", span)?;
    let s = string_arg(args, 0, "nstr_wrap", span)?;
    let width = int_arg(args, 1, "nstr_wrap", span)?;
    if width <= 0 {
        return Err(type_err(span, "nstr_wrap() width must be >= 1".to_string()));
    }
    let width = width as usize;
    let mut out = String::with_capacity(s.len() + 16);
    let mut first_line = true;
    for paragraph in s.split('\n') {
        if !first_line {
            out.push('\n');
        }
        first_line = false;
        let mut col = 0usize;
        let mut first_word = true;
        for word in paragraph.split_whitespace() {
            let wlen = word.chars().count();
            if !first_word && col + 1 + wlen > width {
                out.push('\n');
                col = 0;
            } else if !first_word {
                out.push(' ');
                col += 1;
            }
            out.push_str(word);
            col += wlen;
            first_word = false;
        }
    }
    str_val(out)
}

fn nstr_indent(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_indent", span)?;
    let s = string_arg(args, 0, "nstr_indent", span)?;
    let prefix = string_arg(args, 1, "nstr_indent", span)?;
    let mut out = String::with_capacity(s.len() + prefix.len() * 4);
    for (i, line) in s.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if !line.is_empty() {
            out.push_str(&prefix);
        }
        out.push_str(line);
    }
    str_val(out)
}

fn nstr_dedent(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_dedent", span)?;
    let s = string_arg(args, 0, "nstr_dedent", span)?;
    let mut min_indent: Option<usize> = None;
    for line in s.split('\n') {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        min_indent = Some(match min_indent {
            Some(m) => m.min(indent),
            None => indent,
        });
    }
    let strip = min_indent.unwrap_or(0);
    if strip == 0 {
        return str_val(s);
    }
    let mut out = String::with_capacity(s.len());
    for (i, line) in s.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if line.trim().is_empty() {
            out.push_str(line.trim_start_matches([' ', '\t']));
        } else {
            let stripped: String = line.chars().skip(strip).collect();
            out.push_str(&stripped);
        }
    }
    str_val(out)
}

fn nstr_slugify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nstr_slugify", span)?;
    let s = string_arg(args, 0, "nstr_slugify", span)?;
    let sep = optional_string(args, 1).unwrap_or_else(|| "-".to_string());
    let mut out = String::with_capacity(s.len());
    let mut pending_sep = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push_str(&sep);
            }
            pending_sep = false;
            out.push(c.to_ascii_lowercase());
        } else if c.is_alphanumeric() {
            // non-ASCII letters/digits: keep them lowercased (unicode slugs)
            if pending_sep && !out.is_empty() {
                out.push_str(&sep);
            }
            pending_sep = false;
            out.extend(c.to_lowercase());
        } else {
            pending_sep = true;
        }
    }
    str_val(out)
}

fn levenshtein_impl(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur: Vec<usize> = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn nstr_levenshtein(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_levenshtein", span)?;
    let a = string_arg(args, 0, "nstr_levenshtein", span)?;
    let b = string_arg(args, 1, "nstr_levenshtein", span)?;
    int_val(levenshtein_impl(&a, &b) as i64)
}

fn nstr_similarity(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nstr_similarity", span)?;
    let a = string_arg(args, 0, "nstr_similarity", span)?;
    let b = string_arg(args, 1, "nstr_similarity", span)?;
    let max_len = a.chars().count().max(b.chars().count());
    let sim = if max_len == 0 {
        1.0
    } else {
        1.0 - levenshtein_impl(&a, &b) as f64 / max_len as f64
    };
    Ok(Value::Float(sim).ref_cell())
}

fn check_all(
    args: &[ValueRef],
    name: &str,
    span: Span,
    pred: impl Fn(char) -> bool,
) -> NiaoResult<ValueRef> {
    arity(args, 1, name, span)?;
    let s = string_arg(args, 0, name, span)?;
    bool_val(!s.is_empty() && s.chars().all(pred))
}

fn nstr_is_blank(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_is_blank", span)?;
    let s = string_arg(args, 0, "nstr_is_blank", span)?;
    bool_val(s.trim().is_empty())
}

fn nstr_is_digit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    check_all(args, "nstr_is_digit", span, |c| c.is_ascii_digit())
}

fn nstr_is_alpha(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    check_all(args, "nstr_is_alpha", span, |c| c.is_alphabetic())
}

fn nstr_is_alnum(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    check_all(args, "nstr_is_alnum", span, |c| c.is_alphanumeric())
}

fn nstr_is_upper(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_is_upper", span)?;
    let s = string_arg(args, 0, "nstr_is_upper", span)?;
    let has_alpha = s.chars().any(|c| c.is_alphabetic());
    bool_val(has_alpha && !s.chars().any(|c| c.is_lowercase()))
}

fn nstr_is_lower(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_is_lower", span)?;
    let s = string_arg(args, 0, "nstr_is_lower", span)?;
    let has_alpha = s.chars().any(|c| c.is_alphabetic());
    bool_val(has_alpha && !s.chars().any(|c| c.is_uppercase()))
}

fn nstr_is_ascii(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nstr_is_ascii", span)?;
    let s = string_arg(args, 0, "nstr_is_ascii", span)?;
    bool_val(s.is_ascii())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nstr_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nstr_fns![
    ("nstr_upper", "upper", nstr_upper),
    ("nstr_lower", "lower", nstr_lower),
    ("nstr_title", "title", nstr_title),
    ("nstr_capitalize", "capitalize", nstr_capitalize),
    ("nstr_swap_case", "swap_case", nstr_swap_case),
    ("nstr_snake", "snake", nstr_snake),
    ("nstr_kebab", "kebab", nstr_kebab),
    ("nstr_camel", "camel", nstr_camel),
    ("nstr_pascal", "pascal", nstr_pascal),
    ("nstr_constant", "constant", nstr_constant),
    ("nstr_trim", "trim", nstr_trim),
    ("nstr_trim_start", "trim_start", nstr_trim_start),
    ("nstr_trim_end", "trim_end", nstr_trim_end),
    ("nstr_trim_chars", "trim_chars", nstr_trim_chars),
    ("nstr_pad_start", "pad_start", nstr_pad_start),
    ("nstr_pad_end", "pad_end", nstr_pad_end),
    ("nstr_center", "center", nstr_center),
    ("nstr_repeat", "repeat", nstr_repeat),
    ("nstr_reverse", "reverse", nstr_reverse),
    ("nstr_truncate", "truncate", nstr_truncate),
    ("nstr_split", "split", nstr_split),
    ("nstr_split_n", "split_n", nstr_split_n),
    ("nstr_split_ws", "split_ws", nstr_split_ws),
    ("nstr_join", "join", nstr_join),
    ("nstr_lines", "lines", nstr_lines),
    ("nstr_contains", "contains", nstr_contains),
    ("nstr_starts_with", "starts_with", nstr_starts_with),
    ("nstr_ends_with", "ends_with", nstr_ends_with),
    ("nstr_index_of", "index_of", nstr_index_of),
    ("nstr_last_index_of", "last_index_of", nstr_last_index_of),
    ("nstr_count", "count", nstr_count),
    ("nstr_replace", "replace", nstr_replace),
    ("nstr_replace_n", "replace_n", nstr_replace_n),
    ("nstr_remove_prefix", "remove_prefix", nstr_remove_prefix),
    ("nstr_remove_suffix", "remove_suffix", nstr_remove_suffix),
    ("nstr_substring", "substring", nstr_substring),
    ("nstr_char_at", "char_at", nstr_char_at),
    ("nstr_chars", "chars", nstr_chars),
    ("nstr_char_len", "char_len", nstr_char_len),
    ("nstr_byte_len", "byte_len", nstr_byte_len),
    ("nstr_ord", "ord", nstr_ord),
    ("nstr_chr", "chr", nstr_chr),
    ("nstr_wrap", "wrap", nstr_wrap),
    ("nstr_indent", "indent", nstr_indent),
    ("nstr_dedent", "dedent", nstr_dedent),
    ("nstr_slugify", "slugify", nstr_slugify),
    ("nstr_levenshtein", "levenshtein", nstr_levenshtein),
    ("nstr_similarity", "similarity", nstr_similarity),
    ("nstr_is_blank", "is_blank", nstr_is_blank),
    ("nstr_is_digit", "is_digit", nstr_is_digit),
    ("nstr_is_alpha", "is_alpha", nstr_is_alpha),
    ("nstr_is_alnum", "is_alnum", nstr_is_alnum),
    ("nstr_is_upper", "is_upper", nstr_is_upper),
    ("nstr_is_lower", "is_lower", nstr_is_lower),
    ("nstr_is_ascii", "is_ascii", nstr_is_ascii),
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

pub const MODULE_NAME: &str = "nstr";
pub const MODULE_PATHS: &[&str] = &["nstr", "std/nstr"];

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

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn expect_str(r: NiaoResult<ValueRef>) -> String {
        match &*r.unwrap().borrow() {
            Value::String(v) => v.clone(),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn case_conversions() {
        assert_eq!(
            expect_str(nstr_snake(&[s("HelloWorldHTTP")], span())),
            "hello_world_http"
        );
        assert_eq!(
            expect_str(nstr_snake(&[s("hello-world test")], span())),
            "hello_world_test"
        );
        assert_eq!(
            expect_str(nstr_camel(&[s("hello_world_test")], span())),
            "helloWorldTest"
        );
        assert_eq!(
            expect_str(nstr_pascal(&[s("hello world")], span())),
            "HelloWorld"
        );
        assert_eq!(
            expect_str(nstr_kebab(&[s("HTTPServerV2")], span())),
            "http-server-v2"
        );
        assert_eq!(
            expect_str(nstr_constant(&[s("helloWorld")], span())),
            "HELLO_WORLD"
        );
        assert_eq!(
            expect_str(nstr_title(&[s("hello brave world")], span())),
            "Hello Brave World"
        );
    }

    #[test]
    fn pad_and_center() {
        assert_eq!(
            expect_str(nstr_pad_start(&[s("7"), i(3), s("0")], span())),
            "007"
        );
        assert_eq!(expect_str(nstr_pad_end(&[s("ab"), i(4)], span())), "ab  ");
        assert_eq!(
            expect_str(nstr_center(&[s("hi"), i(6), s("-")], span())),
            "--hi--"
        );
    }

    #[test]
    fn substring_negative_indices() {
        assert_eq!(
            expect_str(nstr_substring(&[s("hello"), i(1), i(3)], span())),
            "el"
        );
        assert_eq!(
            expect_str(nstr_substring(&[s("hello"), i(-3)], span())),
            "llo"
        );
        assert_eq!(
            expect_str(nstr_substring(&[s("héllo"), i(1), i(2)], span())),
            "é"
        );
    }

    #[test]
    fn levenshtein_and_similarity() {
        let d = nstr_levenshtein(&[s("kitten"), s("sitting")], span()).unwrap();
        match &*d.borrow() {
            Value::Int(n) => assert_eq!(*n, 3),
            other => panic!("expected int, got {other:?}"),
        };
    }

    #[test]
    fn wrap_and_dedent() {
        let wrapped = expect_str(nstr_wrap(&[s("one two three four"), i(9)], span()));
        assert_eq!(wrapped, "one two\nthree\nfour");
        let dedented = expect_str(nstr_dedent(&[s("    a\n      b\n    c")], span()));
        assert_eq!(dedented, "a\n  b\nc");
    }

    #[test]
    fn slugify_basic() {
        assert_eq!(
            expect_str(nstr_slugify(&[s("Hello, World! 42")], span())),
            "hello-world-42"
        );
    }

    #[test]
    fn split_join_roundtrip() {
        let parts = nstr_split(&[s("a,b,c"), s(",")], span()).unwrap();
        let joined = nstr_join(&[parts, s("|")], span()).unwrap();
        match &*joined.borrow() {
            Value::String(v) => assert_eq!(v, "a|b|c"),
            other => panic!("expected string, got {other:?}"),
        };
    }

    #[test]
    fn checks() {
        let t = nstr_is_digit(&[s("12345")], span()).unwrap();
        assert!(matches!(&*t.borrow(), Value::Bool(true)));
        let f = nstr_is_digit(&[s("")], span()).unwrap();
        assert!(matches!(&*f.borrow(), Value::Bool(false)));
    }
}
