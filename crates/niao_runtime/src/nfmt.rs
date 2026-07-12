//! Native nfmt standard library — string templating and human-friendly number
//! formatting: `{}` / `{0}` / `{name}` templates, thousands separators, fixed
//! precision, hex/oct/bin, humanized bytes/durations/counts, ordinals.
//!
//! Import with `import "nfmt"` (or `import "std/nfmt"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
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
            codes::E2630_NFMT_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E2630_NFMT_ARITY,
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

fn num_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a number as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
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

fn optional_string(args: &[ValueRef], idx: usize) -> Option<String> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn nfmt_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2631_NFMT_ERROR, "nfmt_error", msg.into(), span)
}

fn str_val(s: String) -> NiaoResult<ValueRef> {
    Ok(Value::String(s).ref_cell())
}

// ---------------------------------------------------------------------------
// Template formatting: {}, {0}, {name}, {{ / }} escapes
// ---------------------------------------------------------------------------

fn nfmt_fmt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.is_empty() {
        return Err(RuntimeError::at(
            span,
            codes::E2630_NFMT_ARITY,
            "nfmt_fmt() expects at least a template string",
        ));
    }
    let template = string_arg(args, 0, "nfmt_fmt", span)?;
    let params = &args[1..];
    // Named lookup uses the last argument when it is an object.
    let named: Option<HashMap<String, ValueRef>> = params.last().and_then(|v| match &*v.borrow() {
        Value::Object(map) => Some(map.clone()),
        _ => None,
    });

    let mut out = String::with_capacity(template.len() + 16);
    let mut auto_index = 0usize;
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '{' {
            if chars.get(i + 1) == Some(&'{') {
                out.push('{');
                i += 2;
                continue;
            }
            let close = match chars[i + 1..].iter().position(|c| *c == '}') {
                Some(off) => i + 1 + off,
                None => {
                    return Ok(nfmt_error(span, "nfmt_fmt() unclosed '{' in template"));
                }
            };
            let key: String = chars[i + 1..close].iter().collect();
            let value: Option<ValueRef> = if key.is_empty() {
                let v = params.get(auto_index).cloned();
                auto_index += 1;
                v
            } else if key.chars().all(|c| c.is_ascii_digit()) {
                let idx: usize = key.parse().unwrap_or(usize::MAX);
                params.get(idx).cloned()
            } else {
                named.as_ref().and_then(|m| m.get(&key).cloned())
            };
            match value {
                Some(v) => out.push_str(&v.borrow().to_string()),
                None => {
                    return Ok(nfmt_error(
                        span,
                        format!("nfmt_fmt() no value for placeholder '{{{key}}}'"),
                    ));
                }
            }
            i = close + 1;
        } else if c == '}' {
            if chars.get(i + 1) == Some(&'}') {
                out.push('}');
                i += 2;
            } else {
                out.push('}');
                i += 1;
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    str_val(out)
}

// ---------------------------------------------------------------------------
// Number formatting
// ---------------------------------------------------------------------------

/// Insert a thousands separator into the integer part of a formatted number.
fn group_digits(digits: &str, sep: &str) -> String {
    let (sign, body) = match digits.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", digits),
    };
    let len = body.len();
    if len <= 3 {
        return format!("{sign}{body}");
    }
    let mut out = String::with_capacity(len + len / 3 * sep.len() + 1);
    out.push_str(sign);
    let first = len % 3;
    if first > 0 {
        out.push_str(&body[..first]);
        if len > first {
            out.push_str(sep);
        }
    }
    for (i, chunk) in body[first..].as_bytes().chunks(3).enumerate() {
        if i > 0 {
            out.push_str(sep);
        }
        out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
    }
    out
}

/// nfmt_number(x, decimals = auto, sep = ",")
fn nfmt_number(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nfmt_number", span)?;
    let sep = optional_string(args, 2).unwrap_or_else(|| ",".to_string());
    let is_int_input = matches!(&*args[0].borrow(), Value::Int(_));
    let decimals = optional_int(args, 1, if is_int_input { 0 } else { 2 });
    if !(0..=15).contains(&decimals) {
        return Err(type_err(span, "nfmt_number() decimals must be in 0..=15"));
    }
    let x = num_arg(args, 0, "nfmt_number", span)?;
    if !x.is_finite() {
        return str_val(x.to_string());
    }
    let formatted = format!("{:.*}", decimals as usize, x);
    let (int_part, frac_part) = match formatted.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (formatted, None),
    };
    let grouped = group_digits(&int_part, &sep);
    match frac_part {
        Some(f) => str_val(format!("{grouped}.{f}")),
        None => str_val(grouped),
    }
}

fn nfmt_fixed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfmt_fixed", span)?;
    let x = num_arg(args, 0, "nfmt_fixed", span)?;
    let decimals = int_arg(args, 1, "nfmt_fixed", span)?;
    if !(0..=17).contains(&decimals) {
        return Err(type_err(span, "nfmt_fixed() decimals must be in 0..=17"));
    }
    str_val(format!("{:.*}", decimals as usize, x))
}

fn nfmt_sci(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfmt_sci", span)?;
    let x = num_arg(args, 0, "nfmt_sci", span)?;
    let decimals = optional_int(args, 1, 3);
    if !(0..=17).contains(&decimals) {
        return Err(type_err(span, "nfmt_sci() decimals must be in 0..=17"));
    }
    str_val(format!("{:.*e}", decimals as usize, x))
}

fn nfmt_percent(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfmt_percent", span)?;
    let x = num_arg(args, 0, "nfmt_percent", span)?;
    let decimals = optional_int(args, 1, 1);
    if !(0..=15).contains(&decimals) {
        return Err(type_err(span, "nfmt_percent() decimals must be in 0..=15"));
    }
    str_val(format!("{:.*}%", decimals as usize, x * 100.0))
}

fn nfmt_currency(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nfmt_currency", span)?;
    let x = num_arg(args, 0, "nfmt_currency", span)?;
    let symbol = string_arg(args, 1, "nfmt_currency", span)?;
    let decimals = optional_int(args, 2, 2);
    if !(0..=6).contains(&decimals) {
        return Err(type_err(span, "nfmt_currency() decimals must be in 0..=6"));
    }
    let formatted = format!("{:.*}", decimals as usize, x.abs());
    let (int_part, frac_part) = match formatted.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (formatted, None),
    };
    let grouped = group_digits(&int_part, ",");
    let sign = if x < 0.0 { "-" } else { "" };
    match frac_part {
        Some(f) => str_val(format!("{sign}{symbol}{grouped}.{f}")),
        None => str_val(format!("{sign}{symbol}{grouped}")),
    }
}

fn based(args: &[ValueRef], name: &str, span: Span, prefix: &str, f: impl Fn(i64) -> String) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, name, span)?;
    let n = int_arg(args, 0, name, span)?;
    let width = optional_int(args, 1, 0);
    if !(0..=64).contains(&width) {
        return Err(type_err(span, format!("{name}() width must be in 0..=64")));
    }
    let body = f(n);
    let padded = if body.len() < width as usize {
        format!("{}{}", "0".repeat(width as usize - body.len()), body)
    } else {
        body
    };
    str_val(format!("{prefix}{padded}"))
}

fn nfmt_hex(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    based(args, "nfmt_hex", span, "0x", |n| format!("{:x}", n as u64))
}

fn nfmt_oct(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    based(args, "nfmt_oct", span, "0o", |n| format!("{:o}", n as u64))
}

fn nfmt_bin(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    based(args, "nfmt_bin", span, "0b", |n| format!("{:b}", n as u64))
}

fn nfmt_ordinal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfmt_ordinal", span)?;
    let n = int_arg(args, 0, "nfmt_ordinal", span)?;
    let suffix = match (n.abs() % 100, n.abs() % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    str_val(format!("{n}{suffix}"))
}

// ---------------------------------------------------------------------------
// Humanizers
// ---------------------------------------------------------------------------

fn humanize_scaled(value: f64, units: &[&str], step: f64) -> String {
    let mut v = value;
    let mut unit = units[0];
    for u in &units[1..] {
        if v.abs() < step {
            break;
        }
        v /= step;
        unit = u;
    }
    if unit == units[0] {
        format!("{} {}", v as i64, unit)
    } else {
        format!("{v:.1} {unit}")
    }
}

/// Decimal bytes: 1 kB = 1000 B.
fn nfmt_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfmt_bytes", span)?;
    let n = num_arg(args, 0, "nfmt_bytes", span)?;
    str_val(humanize_scaled(n, &["B", "kB", "MB", "GB", "TB", "PB"], 1000.0))
}

/// Binary bytes: 1 KiB = 1024 B.
fn nfmt_bytes_bin(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfmt_bytes_bin", span)?;
    let n = num_arg(args, 0, "nfmt_bytes_bin", span)?;
    str_val(humanize_scaled(n, &["B", "KiB", "MiB", "GiB", "TiB", "PiB"], 1024.0))
}

/// Short counts: 1.2k, 3.4M, 5.6B.
fn nfmt_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfmt_count", span)?;
    let n = num_arg(args, 0, "nfmt_count", span)?;
    let abs = n.abs();
    let (v, suffix) = if abs >= 1e12 {
        (n / 1e12, "T")
    } else if abs >= 1e9 {
        (n / 1e9, "B")
    } else if abs >= 1e6 {
        (n / 1e6, "M")
    } else if abs >= 1e3 {
        (n / 1e3, "k")
    } else {
        return str_val(format!("{}", n as i64));
    };
    str_val(format!("{v:.1}{suffix}"))
}

/// Milliseconds → "1d 2h 3m 4s" (largest two units).
fn nfmt_duration_ms(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfmt_duration_ms", span)?;
    let ms = int_arg(args, 0, "nfmt_duration_ms", span)?;
    if ms < 0 {
        return Ok(nfmt_error(span, "nfmt_duration_ms() expects a non-negative duration"));
    }
    if ms < 1000 {
        return str_val(format!("{ms}ms"));
    }
    let total_secs = ms / 1000;
    let days = total_secs / 86_400;
    let hours = (total_secs % 86_400) / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    let parts: Vec<(i64, &str)> = vec![(days, "d"), (hours, "h"), (mins, "m"), (secs, "s")];
    let mut out: Vec<String> = Vec::new();
    for (v, u) in parts {
        if v > 0 || (!out.is_empty() && out.len() < 2) {
            out.push(format!("{v}{u}"));
        }
        if out.len() == 2 {
            break;
        }
    }
    if out.is_empty() {
        out.push("0s".to_string());
    }
    str_val(out.join(" "))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nfmt_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nfmt_fns![
    ("nfmt_fmt", "fmt", nfmt_fmt),
    ("nfmt_number", "number", nfmt_number),
    ("nfmt_fixed", "fixed", nfmt_fixed),
    ("nfmt_sci", "sci", nfmt_sci),
    ("nfmt_percent", "percent", nfmt_percent),
    ("nfmt_currency", "currency", nfmt_currency),
    ("nfmt_hex", "hex", nfmt_hex),
    ("nfmt_oct", "oct", nfmt_oct),
    ("nfmt_bin", "bin", nfmt_bin),
    ("nfmt_ordinal", "ordinal", nfmt_ordinal),
    ("nfmt_bytes", "bytes", nfmt_bytes),
    ("nfmt_bytes_bin", "bytes_bin", nfmt_bytes_bin),
    ("nfmt_count", "count", nfmt_count),
    ("nfmt_duration_ms", "duration_ms", nfmt_duration_ms),
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

pub const MODULE_NAME: &str = "nfmt";
pub const MODULE_PATHS: &[&str] = &["nfmt", "std/nfmt"];

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

    fn f(v: f64) -> ValueRef {
        Value::Float(v).ref_cell()
    }

    fn expect_str(r: NiaoResult<ValueRef>) -> String {
        match &*r.unwrap().borrow() {
            Value::String(v) => v.clone(),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn template_positional_and_named() {
        assert_eq!(
            expect_str(nfmt_fmt(&[s("{} + {} = {}"), i(1), i(2), i(3)], span())),
            "1 + 2 = 3"
        );
        assert_eq!(
            expect_str(nfmt_fmt(&[s("{1} then {0}"), s("a"), s("b")], span())),
            "b then a"
        );
        let mut obj = HashMap::new();
        obj.insert("name".to_string(), Value::String("Niao".into()).ref_cell());
        assert_eq!(
            expect_str(nfmt_fmt(&[s("hi {name}!"), Value::Object(obj).ref_cell()], span())),
            "hi Niao!"
        );
        assert_eq!(expect_str(nfmt_fmt(&[s("{{literal}}")], span())), "{literal}");
    }

    #[test]
    fn thousands_grouping() {
        assert_eq!(expect_str(nfmt_number(&[i(1_234_567)], span())), "1,234,567");
        assert_eq!(expect_str(nfmt_number(&[i(-1000)], span())), "-1,000");
        assert_eq!(expect_str(nfmt_number(&[f(1234.5), i(2)], span())), "1,234.50");
        assert_eq!(expect_str(nfmt_number(&[i(42)], span())), "42");
    }

    #[test]
    fn bases_and_ordinals() {
        assert_eq!(expect_str(nfmt_hex(&[i(255)], span())), "0xff");
        assert_eq!(expect_str(nfmt_hex(&[i(255), i(4)], span())), "0x00ff");
        assert_eq!(expect_str(nfmt_bin(&[i(5)], span())), "0b101");
        assert_eq!(expect_str(nfmt_ordinal(&[i(1)], span())), "1st");
        assert_eq!(expect_str(nfmt_ordinal(&[i(12)], span())), "12th");
        assert_eq!(expect_str(nfmt_ordinal(&[i(23)], span())), "23rd");
    }

    #[test]
    fn humanizers() {
        assert_eq!(expect_str(nfmt_bytes(&[i(1_500_000)], span())), "1.5 MB");
        assert_eq!(expect_str(nfmt_bytes_bin(&[i(1536)], span())), "1.5 KiB");
        assert_eq!(expect_str(nfmt_count(&[i(1_200_000)], span())), "1.2M");
        assert_eq!(expect_str(nfmt_duration_ms(&[i(93_784_000)], span())), "1d 2h");
        assert_eq!(expect_str(nfmt_duration_ms(&[i(500)], span())), "500ms");
        assert_eq!(expect_str(nfmt_duration_ms(&[i(63_000)], span())), "1m 3s");
    }

    #[test]
    fn percent_and_currency() {
        assert_eq!(expect_str(nfmt_percent(&[f(0.425)], span())), "42.5%");
        assert_eq!(expect_str(nfmt_currency(&[f(-1234.5), s("$")], span())), "-$1,234.50");
    }
}
