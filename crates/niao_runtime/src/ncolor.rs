//! Native ncolor standard library — ANSI terminal styling: 16 named colors,
//! bright variants, 256-color and truecolor RGB, text attributes, and strip().
//! Honors the NO_COLOR convention; can be toggled at runtime. When disabled,
//! every function returns its input unchanged (zero-cost pipelines).
//!
//! Import with `import "ncolor"` (or `import "std/ncolor"`).

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Enabled state (NO_COLOR convention, runtime-toggleable)
// ---------------------------------------------------------------------------

fn enabled_flag() -> &'static AtomicBool {
    static ENABLED: OnceLock<AtomicBool> = OnceLock::new();
    ENABLED.get_or_init(|| AtomicBool::new(std::env::var_os("NO_COLOR").is_none()))
}

fn colors_on() -> bool {
    enabled_flag().load(Ordering::Relaxed)
}

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
            codes::E2690_NCOLOR_ARITY,
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

fn color_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E2691_NCOLOR_TYPE, msg.into())
}

fn str_val(s: String) -> NiaoResult<ValueRef> {
    Ok(Value::String(s).ref_cell())
}

// ---------------------------------------------------------------------------
// ANSI core
// ---------------------------------------------------------------------------

const RESET: &str = "\x1b[0m";

fn wrap_codes(s: &str, codes_str: &str) -> String {
    if codes_str.is_empty() {
        return s.to_string();
    }
    format!("\x1b[{codes_str}m{s}{RESET}")
}

/// Named color → base ANSI code (foreground; +10 for background).
fn named_fg(name: &str) -> Option<u8> {
    Some(match name {
        "black" => 30,
        "red" => 31,
        "green" => 32,
        "yellow" => 33,
        "blue" => 34,
        "magenta" => 35,
        "cyan" => 36,
        "white" => 37,
        "gray" | "grey" | "bright_black" => 90,
        "bright_red" => 91,
        "bright_green" => 92,
        "bright_yellow" => 93,
        "bright_blue" => 94,
        "bright_magenta" => 95,
        "bright_cyan" => 96,
        "bright_white" => 97,
        _ => return None,
    })
}

fn attr_code(name: &str) -> Option<u8> {
    Some(match name {
        "bold" => 1,
        "dim" => 2,
        "italic" => 3,
        "underline" => 4,
        "blink" => 5,
        "reverse" => 7,
        "strike" => 9,
        _ => return None,
    })
}

fn check_rgb(v: i64, name: &str, span: Span) -> NiaoResult<u8> {
    u8::try_from(v).map_err(|_| color_err(span, format!("{name}() RGB components must be in 0..=255")))
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

macro_rules! named_color_fn {
    ($fname:ident, $name:literal, $code:literal) => {
        fn $fname(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
            arity(args, 1, $name, span)?;
            let s = string_arg(args, 0, $name, span)?;
            if !colors_on() {
                return str_val(s);
            }
            str_val(wrap_codes(&s, $code))
        }
    };
}

named_color_fn!(ncolor_black, "ncolor_black", "30");
named_color_fn!(ncolor_red, "ncolor_red", "31");
named_color_fn!(ncolor_green, "ncolor_green", "32");
named_color_fn!(ncolor_yellow, "ncolor_yellow", "33");
named_color_fn!(ncolor_blue, "ncolor_blue", "34");
named_color_fn!(ncolor_magenta, "ncolor_magenta", "35");
named_color_fn!(ncolor_cyan, "ncolor_cyan", "36");
named_color_fn!(ncolor_white, "ncolor_white", "37");
named_color_fn!(ncolor_gray, "ncolor_gray", "90");
named_color_fn!(ncolor_bold, "ncolor_bold", "1");
named_color_fn!(ncolor_dim, "ncolor_dim", "2");
named_color_fn!(ncolor_italic, "ncolor_italic", "3");
named_color_fn!(ncolor_underline, "ncolor_underline", "4");
named_color_fn!(ncolor_strike, "ncolor_strike", "9");
named_color_fn!(ncolor_reverse, "ncolor_reverse", "7");

/// ncolor_fg(text, color_name)
fn ncolor_fg(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncolor_fg", span)?;
    let s = string_arg(args, 0, "ncolor_fg", span)?;
    let name = string_arg(args, 1, "ncolor_fg", span)?;
    let Some(code) = named_fg(&name) else {
        return Err(color_err(span, format!("unknown color '{name}'")));
    };
    if !colors_on() {
        return str_val(s);
    }
    str_val(wrap_codes(&s, &code.to_string()))
}

/// ncolor_bg(text, color_name)
fn ncolor_bg(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncolor_bg", span)?;
    let s = string_arg(args, 0, "ncolor_bg", span)?;
    let name = string_arg(args, 1, "ncolor_bg", span)?;
    let Some(code) = named_fg(&name) else {
        return Err(color_err(span, format!("unknown color '{name}'")));
    };
    if !colors_on() {
        return str_val(s);
    }
    str_val(wrap_codes(&s, &(code + 10).to_string()))
}

/// ncolor_rgb(text, r, g, b) — truecolor foreground.
fn ncolor_rgb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "ncolor_rgb", span)?;
    let s = string_arg(args, 0, "ncolor_rgb", span)?;
    let r = check_rgb(int_arg(args, 1, "ncolor_rgb", span)?, "ncolor_rgb", span)?;
    let g = check_rgb(int_arg(args, 2, "ncolor_rgb", span)?, "ncolor_rgb", span)?;
    let b = check_rgb(int_arg(args, 3, "ncolor_rgb", span)?, "ncolor_rgb", span)?;
    if !colors_on() {
        return str_val(s);
    }
    str_val(wrap_codes(&s, &format!("38;2;{r};{g};{b}")))
}

/// ncolor_on_rgb(text, r, g, b) — truecolor background.
fn ncolor_on_rgb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "ncolor_on_rgb", span)?;
    let s = string_arg(args, 0, "ncolor_on_rgb", span)?;
    let r = check_rgb(int_arg(args, 1, "ncolor_on_rgb", span)?, "ncolor_on_rgb", span)?;
    let g = check_rgb(int_arg(args, 2, "ncolor_on_rgb", span)?, "ncolor_on_rgb", span)?;
    let b = check_rgb(int_arg(args, 3, "ncolor_on_rgb", span)?, "ncolor_on_rgb", span)?;
    if !colors_on() {
        return str_val(s);
    }
    str_val(wrap_codes(&s, &format!("48;2;{r};{g};{b}")))
}

/// ncolor_c256(text, index) — 256-color foreground.
fn ncolor_c256(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncolor_c256", span)?;
    let s = string_arg(args, 0, "ncolor_c256", span)?;
    let idx = int_arg(args, 1, "ncolor_c256", span)?;
    let idx = u8::try_from(idx)
        .map_err(|_| color_err(span, "ncolor_c256() index must be in 0..=255"))?;
    if !colors_on() {
        return str_val(s);
    }
    str_val(wrap_codes(&s, &format!("38;5;{idx}")))
}

/// ncolor_style(text, opts) — composite: {fg, bg, bold, underline, ...}.
/// fg/bg accept a color name, a 256-palette int, or an [r, g, b] array.
fn ncolor_style(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncolor_style", span)?;
    let s = string_arg(args, 0, "ncolor_style", span)?;
    let opts = match &*args[1].borrow() {
        Value::Object(map) => map.clone(),
        other => {
            return Err(type_err(
                span,
                format!("ncolor_style() expects an options object, got {}", other.type_name()),
            ))
        }
    };
    if !colors_on() {
        return str_val(s);
    }
    let mut parts: Vec<String> = Vec::new();
    // attributes first, sorted for deterministic output
    let mut attr_names: Vec<&String> = opts.keys().collect();
    attr_names.sort();
    for key in attr_names {
        if let Some(code) = attr_code(key) {
            if matches!(&*opts.get(key).unwrap().borrow(), Value::Bool(true)) {
                parts.push(code.to_string());
            }
        }
    }
    for (key, base) in [("fg", 0u8), ("bg", 10u8)] {
        let Some(v) = opts.get(key) else {
            continue;
        };
        match &*v.borrow() {
            Value::String(name) => {
                let Some(code) = named_fg(name) else {
                    return Err(color_err(span, format!("unknown color '{name}'")));
                };
                parts.push((code + base).to_string());
            }
            Value::Int(idx) => {
                let idx = u8::try_from(*idx)
                    .map_err(|_| color_err(span, "ncolor_style() palette index must be in 0..=255"))?;
                let selector = if base == 0 { 38 } else { 48 };
                parts.push(format!("{selector};5;{idx}"));
            }
            Value::Array(rgb) if rgb.len() == 3 => {
                let mut c = [0u8; 3];
                for (i, comp) in rgb.iter().enumerate() {
                    match &*comp.borrow() {
                        Value::Int(n) => {
                            c[i] = check_rgb(*n, "ncolor_style", span)?;
                        }
                        other => {
                            return Err(type_err(
                                span,
                                format!("ncolor_style() RGB must be ints, found {}", other.type_name()),
                            ))
                        }
                    }
                }
                let selector = if base == 0 { 38 } else { 48 };
                parts.push(format!("{selector};2;{};{};{}", c[0], c[1], c[2]));
            }
            Value::Nil => {}
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "ncolor_style() {key} must be a color name, int, or [r,g,b], got {}",
                        other.type_name()
                    ),
                ))
            }
        }
    }
    str_val(wrap_codes(&s, &parts.join(";")))
}

/// Remove all ANSI escape sequences.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for t in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&t) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn ncolor_strip(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncolor_strip", span)?;
    let s = string_arg(args, 0, "ncolor_strip", span)?;
    str_val(strip_ansi(&s))
}

fn ncolor_set_enabled(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncolor_set_enabled", span)?;
    let on = match &*args[0].borrow() {
        Value::Bool(b) => *b,
        other => {
            return Err(type_err(
                span,
                format!("ncolor_set_enabled() expects a bool, got {}", other.type_name()),
            ))
        }
    };
    enabled_flag().store(on, Ordering::Relaxed);
    Ok(Value::Nil.ref_cell())
}

fn ncolor_is_enabled(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncolor_is_enabled", span)?;
    Ok(Value::Bool(colors_on()).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncolor_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncolor_fns![
    ("ncolor_black", "black", ncolor_black),
    ("ncolor_red", "red", ncolor_red),
    ("ncolor_green", "green", ncolor_green),
    ("ncolor_yellow", "yellow", ncolor_yellow),
    ("ncolor_blue", "blue", ncolor_blue),
    ("ncolor_magenta", "magenta", ncolor_magenta),
    ("ncolor_cyan", "cyan", ncolor_cyan),
    ("ncolor_white", "white", ncolor_white),
    ("ncolor_gray", "gray", ncolor_gray),
    ("ncolor_bold", "bold", ncolor_bold),
    ("ncolor_dim", "dim", ncolor_dim),
    ("ncolor_italic", "italic", ncolor_italic),
    ("ncolor_underline", "underline", ncolor_underline),
    ("ncolor_strike", "strike", ncolor_strike),
    ("ncolor_reverse", "reverse", ncolor_reverse),
    ("ncolor_fg", "fg", ncolor_fg),
    ("ncolor_bg", "bg", ncolor_bg),
    ("ncolor_rgb", "rgb", ncolor_rgb),
    ("ncolor_on_rgb", "on_rgb", ncolor_on_rgb),
    ("ncolor_c256", "c256", ncolor_c256),
    ("ncolor_style", "style", ncolor_style),
    ("ncolor_strip", "strip", ncolor_strip),
    ("ncolor_set_enabled", "set_enabled", ncolor_set_enabled),
    ("ncolor_is_enabled", "is_enabled", ncolor_is_enabled),
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

pub const MODULE_NAME: &str = "ncolor";
pub const MODULE_PATHS: &[&str] = &["ncolor", "std/ncolor"];

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

    fn expect_str(r: NiaoResult<ValueRef>) -> String {
        match &*r.unwrap().borrow() {
            Value::String(v) => v.clone(),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn red_wraps_when_enabled() {
        enabled_flag().store(true, Ordering::Relaxed);
        assert_eq!(expect_str(ncolor_red(&[s("hi")], span())), "\x1b[31mhi\x1b[0m");
    }

    #[test]
    fn disabled_passthrough() {
        enabled_flag().store(false, Ordering::Relaxed);
        assert_eq!(expect_str(ncolor_red(&[s("hi")], span())), "hi");
        enabled_flag().store(true, Ordering::Relaxed);
    }

    #[test]
    fn strip_removes_codes() {
        enabled_flag().store(true, Ordering::Relaxed);
        let colored = expect_str(ncolor_rgb(&[s("x"), Value::Int(1).ref_cell(), Value::Int(2).ref_cell(), Value::Int(3).ref_cell()], span()));
        assert_eq!(strip_ansi(&colored), "x");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn style_composite() {
        enabled_flag().store(true, Ordering::Relaxed);
        let mut opts = HashMap::new();
        opts.insert("bold".to_string(), Value::Bool(true).ref_cell());
        opts.insert("fg".to_string(), Value::String("red".into()).ref_cell());
        let out = expect_str(ncolor_style(&[s("t"), Value::Object(opts).ref_cell()], span()));
        assert_eq!(out, "\x1b[1;31mt\x1b[0m");
    }
}
