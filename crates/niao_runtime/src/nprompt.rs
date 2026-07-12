//! Native nprompt standard library — interactive TTY prompts on stdin/stdout.
//! Falls back to plain line reads when stdin is not a terminal (pipes, redirects).
//!
//! Import with `import "nprompt"` (or `import "std/nprompt"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::io::{self, stdin, stdout, IsTerminal, Write};
use std::rc::Rc;

// Wired into niao_errors::codes by central integration.
const E2920_NPROMPT_ARITY: u32 = 2920;
const E2921_NPROMPT_ERROR: u32 = 2921;
const E2922_NPROMPT_TYPE: u32 = 2922;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E2922_NPROMPT_TYPE, msg.into())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E2920_NPROMPT_ARITY,
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

fn optional_object_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Option<HashMap<String, ValueRef>>> {
    if args.len() <= idx {
        return Ok(None);
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(Some(map.clone())),
        Value::Nil => Ok(None),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an options object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn string_array_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<String>> {
    fn collect_strings(items: &[ValueRef], name: &str, span: Span) -> NiaoResult<Vec<String>> {
        let mut out = Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            match &*item.borrow() {
                Value::String(s) => out.push(s.clone()),
                other => {
                    return Err(type_err(
                        span,
                        format!(
                            "{name}() expects string choices; item {} is {}",
                            i + 1,
                            other.type_name()
                        ),
                    ));
                }
            }
        }
        Ok(out)
    }

    match &*args[idx].borrow() {
        Value::Array(items) => collect_strings(items, name, span),
        Value::StringArray(items) => Ok(items.dense_vec()),
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

fn prompt_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2921_NPROMPT_ERROR, "nprompt_error", msg.into(), span)
}

fn str_val(s: String) -> NiaoResult<ValueRef> {
    Ok(Value::String(s).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

// ---------------------------------------------------------------------------
// TTY / styling (optional — honors NO_COLOR)
// ---------------------------------------------------------------------------

fn colors_on() -> bool {
    std::env::var_os("NO_COLOR").is_none()
}

fn is_interactive() -> bool {
    stdin().is_terminal() && stdout().is_terminal()
}

fn dim(s: &str) -> String {
    if colors_on() && is_interactive() {
        format!("\x1b[2m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

fn bold(s: &str) -> String {
    if colors_on() && is_interactive() {
        format!("\x1b[1m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// I/O helpers
// ---------------------------------------------------------------------------

fn flush_prompt(text: &str) -> Result<(), io::Error> {
    stdout().write_all(text.as_bytes())?;
    stdout().flush()
}

fn read_line() -> Result<String, io::Error> {
    let mut line = String::new();
    stdin().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

fn read_line_prompt(prompt: &str) -> Result<String, io::Error> {
    flush_prompt(prompt)?;
    read_line()
}

#[cfg(unix)]
fn read_password_line(prompt: &str) -> Result<String, io::Error> {
    use std::process::Stdio;

    flush_prompt(prompt)?;

    if !is_interactive() {
        return read_line();
    }

    // Best-effort no-echo via stty (common on Unix TTYs).
    let stty_ok = std::process::Command::new("stty")
        .arg("-echo")
        .stdin(Stdio::inherit())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !stty_ok {
        return read_line();
    }

    let result = read_line();
    let _ = std::process::Command::new("stty")
        .arg("echo")
        .stdin(Stdio::inherit())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = stdout().write_all(b"\n");
    let _ = stdout().flush();
    result
}

#[cfg(not(unix))]
fn read_password_line(prompt: &str) -> Result<String, io::Error> {
    // Windows: std has no portable no-echo API — read with echo (best effort).
    read_line_prompt(prompt)
}

// ---------------------------------------------------------------------------
// Parsing helpers (unit-tested)
// ---------------------------------------------------------------------------

/// Parse y/n confirm input. Empty line uses `default`; invalid input returns `None`.
fn parse_confirm_response(input: &str, default: Option<bool>) -> Option<bool> {
    let t = input.trim();
    if t.is_empty() {
        return default;
    }
    match t.to_ascii_lowercase().as_str() {
        "y" | "yes" => Some(true),
        "n" | "no" => Some(false),
        _ => None,
    }
}

fn confirm_hint(default: Option<bool>) -> &'static str {
    match default {
        Some(true) => " [Y/n] ",
        Some(false) => " [y/N] ",
        None => " [y/n] ",
    }
}

/// Resolve a select response in non-TTY mode: numeric input → index, else matching choice string.
fn parse_select_pipe(input: &str, choices: &[String], default_index: Option<usize>) -> Option<ValueRef> {
    let t = input.trim();
    if t.is_empty() {
        return default_index.and_then(|i| choices.get(i).map(|s| Value::String(s.clone()).ref_cell()));
    }
    if let Ok(n) = t.parse::<i64>() {
        if n >= 0 && (n as usize) < choices.len() {
            return Some(Value::Int(n).ref_cell());
        }
    }
    if let Some(pos) = choices.iter().position(|c| c == t) {
        return Some(Value::String(choices[pos].clone()).ref_cell());
    }
    None
}

/// Resolve a 1-based menu selection in TTY mode.
fn parse_select_tty(input: &str, choices: &[String], default_index: Option<usize>) -> Option<String> {
    let t = input.trim();
    if t.is_empty() {
        return default_index.and_then(|i| choices.get(i).cloned());
    }
    if let Ok(n) = t.parse::<usize>() {
        if n >= 1 && n <= choices.len() {
            return Some(choices[n - 1].clone());
        }
    }
    if choices.iter().any(|c| c == t) {
        return Some(t.to_string());
    }
    None
}

fn opts_string(opts: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<String> {
    let map = opts?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    }
}

fn opts_bool(opts: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<bool> {
    let map = opts?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => Some(b),
        _ => None,
    }
}

fn opts_usize(opts: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<usize> {
    let map = opts?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) if n >= 0 => Some(n as usize),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nprompt_input(label, opts?) → string
fn nprompt_input(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nprompt_input", span)?;
    let label = string_arg(args, 0, "nprompt_input", span)?;
    let opts = optional_object_arg(args, 1, "nprompt_input", span)?;
    let default = opts_string(opts.as_ref(), "default");

    let prompt = if is_interactive() {
        if let Some(ref d) = default {
            format!("{}{} ", bold(&label), dim(&format!("[{d}]")))
        } else {
            format!("{} ", bold(&label))
        }
    } else if let Some(ref d) = default {
        format!("{label} [{d}]: ")
    } else {
        format!("{label}: ")
    };

    let line = match read_line_prompt(&prompt) {
        Ok(s) => s,
        Err(e) => return Ok(prompt_err(span, format!("stdin read failed: {e}"))),
    };

    if line.is_empty() {
        if let Some(d) = default {
            return str_val(d);
        }
    }
    str_val(line)
}

/// nprompt_confirm(label, opts?) → bool
fn nprompt_confirm(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nprompt_confirm", span)?;
    let label = string_arg(args, 0, "nprompt_confirm", span)?;
    let opts = optional_object_arg(args, 1, "nprompt_confirm", span)?;
    let default = opts_bool(opts.as_ref(), "default");

    let prompt = if is_interactive() {
        format!("{}{}", bold(&label), dim(confirm_hint(default)))
    } else {
        let hint = match default {
            Some(true) => " [Y/n]",
            Some(false) => " [y/N]",
            None => " [y/n]",
        };
        format!("{label}{hint}: ")
    };

    if !is_interactive() {
        let line = match read_line_prompt(&prompt) {
            Ok(s) => s,
            Err(e) => return Ok(prompt_err(span, format!("stdin read failed: {e}"))),
        };
        return match parse_confirm_response(&line, default) {
            Some(b) => bool_val(b),
            None => Ok(prompt_err(
                span,
                format!("invalid confirm response '{line}'; expected y/n"),
            )),
        };
    }

    loop {
        let line = match read_line_prompt(&prompt) {
            Ok(s) => s,
            Err(e) => return Ok(prompt_err(span, format!("stdin read failed: {e}"))),
        };
        if let Some(b) = parse_confirm_response(&line, default) {
            return bool_val(b);
        }
        let _ = flush_prompt(&dim("Please enter y or n. "));
    }
}

/// nprompt_select(label, choices[], opts?) → string or int index
fn nprompt_select(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nprompt_select", span)?;
    let label = string_arg(args, 0, "nprompt_select", span)?;
    let choices = string_array_arg(args, 1, "nprompt_select", span)?;
    if choices.is_empty() {
        return Ok(prompt_err(span, "nprompt_select() requires at least one choice"));
    }
    let opts = optional_object_arg(args, 2, "nprompt_select", span)?;
    let default_index = opts_usize(opts.as_ref(), "default_index");

    if default_index.is_some_and(|i| i >= choices.len()) {
        return Ok(prompt_err(
            span,
            format!(
                "default_index {} out of range (0..={})",
                default_index.unwrap(),
                choices.len() - 1
            ),
        ));
    }

    if is_interactive() {
        let mut prompt = format!("{}\n", bold(&label));
        for (i, choice) in choices.iter().enumerate() {
            let marker = if default_index == Some(i) { "*" } else { " " };
            prompt.push_str(&format!("  {marker}{} ) {}\n", i + 1, choice));
        }
        prompt.push_str(&dim("Enter choice number: "));
        loop {
            let line = match read_line_prompt(&prompt) {
                Ok(s) => s,
                Err(e) => return Ok(prompt_err(span, format!("stdin read failed: {e}"))),
            };
            if let Some(selected) = parse_select_tty(&line, &choices, default_index) {
                return str_val(selected);
            }
            prompt = dim("Invalid choice. Enter choice number: ").to_string();
        }
    }

    let prompt = format!("{label}: ");
    let line = match read_line_prompt(&prompt) {
        Ok(s) => s,
        Err(e) => return Ok(prompt_err(span, format!("stdin read failed: {e}"))),
    };
    if let Some(v) = parse_select_pipe(&line, &choices, default_index) {
        return Ok(v);
    }
    Ok(prompt_err(
        span,
        format!("invalid selection '{line}'; expected index 0..={} or a choice label", choices.len() - 1),
    ))
}

/// nprompt_password(label) → string (no echo on Unix TTY, best effort elsewhere)
fn nprompt_password(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nprompt_password", span)?;
    let label = string_arg(args, 0, "nprompt_password", span)?;
    let prompt = if is_interactive() {
        format!("{} ", bold(&label))
    } else {
        format!("{label}: ")
    };
    match read_password_line(&prompt) {
        Ok(s) => str_val(s),
        Err(e) => Ok(prompt_err(span, format!("stdin read failed: {e}"))),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nprompt_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nprompt_fns![
    ("nprompt_input", "input", nprompt_input),
    ("nprompt_confirm", "confirm", nprompt_confirm),
    ("nprompt_select", "select", nprompt_select),
    ("nprompt_password", "password", nprompt_password),
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

pub const MODULE_NAME: &str = "nprompt";
pub const MODULE_PATHS: &[&str] = &["nprompt", "std/nprompt"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_yes_variants() {
        assert_eq!(parse_confirm_response("y", None), Some(true));
        assert_eq!(parse_confirm_response("Y", None), Some(true));
        assert_eq!(parse_confirm_response("yes", None), Some(true));
        assert_eq!(parse_confirm_response("YES", None), Some(true));
        assert_eq!(parse_confirm_response("  yes  ", None), Some(true));
    }

    #[test]
    fn confirm_no_variants() {
        assert_eq!(parse_confirm_response("n", None), Some(false));
        assert_eq!(parse_confirm_response("N", None), Some(false));
        assert_eq!(parse_confirm_response("no", None), Some(false));
        assert_eq!(parse_confirm_response("NO", None), Some(false));
    }

    #[test]
    fn confirm_empty_uses_default() {
        assert_eq!(parse_confirm_response("", Some(true)), Some(true));
        assert_eq!(parse_confirm_response("", Some(false)), Some(false));
        assert_eq!(parse_confirm_response("", None), None);
    }

    #[test]
    fn confirm_invalid() {
        assert_eq!(parse_confirm_response("maybe", None), None);
        assert_eq!(parse_confirm_response("1", None), None);
        assert_eq!(parse_confirm_response("true", None), None);
    }

    #[test]
    fn confirm_hint_labels() {
        assert_eq!(confirm_hint(Some(true)), " [Y/n] ");
        assert_eq!(confirm_hint(Some(false)), " [y/N] ");
        assert_eq!(confirm_hint(None), " [y/n] ");
    }

    #[test]
    fn select_tty_parsing() {
        let choices = vec!["red".into(), "green".into(), "blue".into()];
        assert_eq!(parse_select_tty("2", &choices, None), Some("green".into()));
        assert_eq!(parse_select_tty("", &choices, Some(0)), Some("red".into()));
        assert_eq!(parse_select_tty("blue", &choices, None), Some("blue".into()));
        assert_eq!(parse_select_tty("9", &choices, None), None);
        assert_eq!(parse_select_tty("purple", &choices, None), None);
    }

    #[test]
    fn select_pipe_parsing() {
        let choices = vec!["alpha".into(), "beta".into()];
        match &*parse_select_pipe("1", &choices, None).unwrap().borrow() {
            Value::Int(1) => {}
            other => panic!("expected int 1, got {other:?}"),
        }
        match &*parse_select_pipe("beta", &choices, None).unwrap().borrow() {
            Value::String(s) if s == "beta" => {}
            other => panic!("expected string beta, got {other:?}"),
        }
        assert!(parse_select_pipe("", &choices, Some(0)).is_some());
        assert!(parse_select_pipe("nope", &choices, None).is_none());
    }
}
