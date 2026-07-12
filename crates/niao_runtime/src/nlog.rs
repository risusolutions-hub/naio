//! Native nlog standard library — lightweight structured logging with levels,
//! key-value fields, text or JSON output, and stderr/stdout/file sinks.
//! One atomic level check gates all work, so disabled log calls cost ~1 ns.
//!
//! Import with `import "nlog"` (or `import "std/nlog"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::rc::Rc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Levels
// ---------------------------------------------------------------------------

const LEVEL_TRACE: u8 = 0;
const LEVEL_DEBUG: u8 = 1;
const LEVEL_INFO: u8 = 2;
const LEVEL_WARN: u8 = 3;
const LEVEL_ERROR: u8 = 4;
const LEVEL_OFF: u8 = 5;

/// Fast-path level gate (mirrors `LogState.level`).
static CURRENT_LEVEL: AtomicU8 = AtomicU8::new(LEVEL_INFO);

fn level_from_str(s: &str) -> Option<u8> {
    match s.to_ascii_lowercase().as_str() {
        "trace" => Some(LEVEL_TRACE),
        "debug" => Some(LEVEL_DEBUG),
        "info" => Some(LEVEL_INFO),
        "warn" | "warning" => Some(LEVEL_WARN),
        "error" => Some(LEVEL_ERROR),
        "off" | "none" => Some(LEVEL_OFF),
        _ => None,
    }
}

fn level_name(level: u8) -> &'static str {
    match level {
        LEVEL_TRACE => "trace",
        LEVEL_DEBUG => "debug",
        LEVEL_INFO => "info",
        LEVEL_WARN => "warn",
        LEVEL_ERROR => "error",
        _ => "off",
    }
}

fn level_label(level: u8) -> &'static str {
    match level {
        LEVEL_TRACE => "TRACE",
        LEVEL_DEBUG => "DEBUG",
        LEVEL_INFO => "INFO",
        LEVEL_WARN => "WARN",
        LEVEL_ERROR => "ERROR",
        _ => "OFF",
    }
}

// ---------------------------------------------------------------------------
// Global logger state
// ---------------------------------------------------------------------------

enum Sink {
    Stderr,
    Stdout,
    File(File),
}

struct LogState {
    format_json: bool,
    timestamps: bool,
    sink: Sink,
    /// Global context fields prepended to every record.
    context: Vec<(String, FieldValue)>,
}

/// Log field values captured as owned, thread-safe data.
#[derive(Clone)]
enum FieldValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

impl FieldValue {
    fn from_value(v: &Value) -> FieldValue {
        match v {
            Value::Int(n) => FieldValue::Int(*n),
            Value::Float(f) => FieldValue::Float(*f),
            Value::Bool(b) => FieldValue::Bool(*b),
            Value::String(s) => FieldValue::Str(s.clone()),
            other => FieldValue::Str(other.to_string()),
        }
    }

    fn text(&self) -> String {
        match self {
            FieldValue::Int(n) => n.to_string(),
            FieldValue::Float(f) => f.to_string(),
            FieldValue::Bool(b) => b.to_string(),
            FieldValue::Str(s) => s.clone(),
        }
    }

    fn json(&self) -> String {
        match self {
            FieldValue::Int(n) => n.to_string(),
            FieldValue::Float(f) => {
                if f.is_finite() {
                    f.to_string()
                } else {
                    format!("\"{f}\"")
                }
            }
            FieldValue::Bool(b) => b.to_string(),
            FieldValue::Str(s) => json_escape(s),
        }
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn state() -> &'static Mutex<LogState> {
    static STATE: std::sync::OnceLock<Mutex<LogState>> = std::sync::OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(LogState {
            format_json: false,
            timestamps: true,
            sink: Sink::Stderr,
            context: Vec::new(),
        })
    })
}

// ---------------------------------------------------------------------------
// Timestamp (ISO-8601 UTC, no chrono needed)
// ---------------------------------------------------------------------------

/// Days-since-epoch → (year, month, day). Howard Hinnant's civil_from_days.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn iso_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;
    let millis = now.subsec_millis();
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, mo, d) = civil_from_days(days);
    let h = rem / 3600;
    let mi = (rem % 3600) / 60;
    let s = rem % 60;
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{millis:03}Z")
}

// ---------------------------------------------------------------------------
// Record emission
// ---------------------------------------------------------------------------

fn emit(level: u8, msg: &str, fields: &[(String, FieldValue)], span: Span) -> NiaoResult<ValueRef> {
    let mut st = state().lock().map_err(|_| {
        RuntimeError::at(span, codes::E2641_NLOG_ERROR, "nlog state lock poisoned")
    })?;
    let mut line = String::with_capacity(64 + msg.len() + fields.len() * 16);
    if st.format_json {
        line.push('{');
        if st.timestamps {
            line.push_str("\"ts\":");
            line.push_str(&json_escape(&iso_timestamp()));
            line.push(',');
        }
        line.push_str("\"level\":");
        line.push_str(&json_escape(level_name(level)));
        line.push_str(",\"msg\":");
        line.push_str(&json_escape(msg));
        for (k, v) in st.context.iter().chain(fields.iter()) {
            line.push(',');
            line.push_str(&json_escape(k));
            line.push(':');
            line.push_str(&v.json());
        }
        line.push('}');
    } else {
        if st.timestamps {
            line.push_str(&iso_timestamp());
            line.push(' ');
        }
        line.push_str(level_label(level));
        line.push(' ');
        line.push_str(msg);
        for (k, v) in st.context.iter().chain(fields.iter()) {
            line.push(' ');
            line.push_str(k);
            line.push('=');
            let text = v.text();
            if text.contains(' ') || text.is_empty() {
                line.push('"');
                line.push_str(&text);
                line.push('"');
            } else {
                line.push_str(&text);
            }
        }
    }
    line.push('\n');
    let result = match &mut st.sink {
        Sink::Stderr => std::io::stderr().write_all(line.as_bytes()),
        Sink::Stdout => std::io::stdout().write_all(line.as_bytes()),
        Sink::File(f) => f.write_all(line.as_bytes()).and_then(|_| f.flush()),
    };
    if let Err(e) = result {
        return Ok(error_value(
            codes::E2641_NLOG_ERROR,
            "nlog_error",
            format!("nlog sink write failed: {e}"),
            span,
        ));
    }
    Ok(Value::Nil.ref_cell())
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

fn nlog_error_val(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2641_NLOG_ERROR, "nlog_error", msg.into(), span)
}

/// Collect trailing key/value varargs into owned field pairs.
fn collect_fields(args: &[ValueRef], start: usize, name: &str, span: Span) -> NiaoResult<Vec<(String, FieldValue)>> {
    let rest = &args[start..];
    if rest.len() % 2 != 0 {
        return Err(RuntimeError::at(
            span,
            codes::E2640_NLOG_ARITY,
            format!("{name}() expects key/value pairs after the message, got an odd number"),
        ));
    }
    let mut fields = Vec::with_capacity(rest.len() / 2);
    for pair in rest.chunks(2) {
        let key = match &*pair[0].borrow() {
            Value::String(s) => s.clone(),
            other => {
                return Err(type_err(
                    span,
                    format!("{name}() field keys must be strings, got {}", other.type_name()),
                ))
            }
        };
        let value = FieldValue::from_value(&pair[1].borrow());
        fields.push((key, value));
    }
    Ok(fields)
}

fn log_at(level: u8, name: &str, args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if level < CURRENT_LEVEL.load(Ordering::Relaxed) {
        return Ok(Value::Nil.ref_cell());
    }
    if args.is_empty() {
        return Err(RuntimeError::at(
            span,
            codes::E2640_NLOG_ARITY,
            format!("{name}() expects at least a message"),
        ));
    }
    let msg = match &*args[0].borrow() {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let fields = collect_fields(args, 1, name, span)?;
    emit(level, &msg, &fields, span)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nlog_init(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() > 2 {
        return Err(RuntimeError::at(
            span,
            codes::E2640_NLOG_ARITY,
            "nlog_init() expects 0..=2 arguments (level, options)",
        ));
    }
    let level_str = if args.is_empty() {
        "info".to_string()
    } else {
        string_arg(args, 0, "nlog_init", span)?
    };
    let Some(level) = level_from_str(&level_str) else {
        return Ok(nlog_error_val(span, format!("unknown log level '{level_str}'")));
    };
    let opts: Option<HashMap<String, ValueRef>> = args.get(1).and_then(|v| match &*v.borrow() {
        Value::Object(map) => Some(map.clone()),
        _ => None,
    });
    let mut format_json = false;
    let mut timestamps = true;
    let mut file_path: Option<String> = None;
    let mut to_stdout = false;
    if let Some(opts) = &opts {
        if let Some(v) = opts.get("format") {
            if let Value::String(s) = &*v.borrow() {
                match s.as_str() {
                    "json" => format_json = true,
                    "text" => format_json = false,
                    other => {
                        return Ok(nlog_error_val(span, format!("unknown log format '{other}'")))
                    }
                }
            }
        }
        if let Some(v) = opts.get("timestamps") {
            if let Value::Bool(b) = &*v.borrow() {
                timestamps = *b;
            }
        }
        if let Some(v) = opts.get("file") {
            if let Value::String(s) = &*v.borrow() {
                file_path = Some(s.clone());
            }
        }
        if let Some(v) = opts.get("stdout") {
            if let Value::Bool(true) = &*v.borrow() {
                to_stdout = true;
            }
        }
    }
    let sink = if let Some(path) = file_path {
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(f) => Sink::File(f),
            Err(e) => {
                return Ok(nlog_error_val(span, format!("cannot open log file '{path}': {e}")))
            }
        }
    } else if to_stdout {
        Sink::Stdout
    } else {
        Sink::Stderr
    };
    let mut st = state().lock().map_err(|_| {
        RuntimeError::at(span, codes::E2641_NLOG_ERROR, "nlog state lock poisoned")
    })?;
    st.format_json = format_json;
    st.timestamps = timestamps;
    st.sink = sink;
    CURRENT_LEVEL.store(level, Ordering::Relaxed);
    Ok(Value::Nil.ref_cell())
}

fn nlog_set_level(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            codes::E2640_NLOG_ARITY,
            "nlog_set_level() expects 1 argument",
        ));
    }
    let level_str = string_arg(args, 0, "nlog_set_level", span)?;
    match level_from_str(&level_str) {
        Some(level) => {
            CURRENT_LEVEL.store(level, Ordering::Relaxed);
            Ok(Value::Nil.ref_cell())
        }
        None => Ok(nlog_error_val(span, format!("unknown log level '{level_str}'"))),
    }
}

fn nlog_get_level(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if !args.is_empty() {
        return Err(RuntimeError::at(
            span,
            codes::E2640_NLOG_ARITY,
            "nlog_get_level() expects 0 arguments",
        ));
    }
    Ok(Value::String(level_name(CURRENT_LEVEL.load(Ordering::Relaxed)).to_string()).ref_cell())
}

fn nlog_enabled(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            codes::E2640_NLOG_ARITY,
            "nlog_enabled() expects 1 argument",
        ));
    }
    let level_str = string_arg(args, 0, "nlog_enabled", span)?;
    match level_from_str(&level_str) {
        Some(level) => {
            Ok(Value::Bool(level >= CURRENT_LEVEL.load(Ordering::Relaxed)).ref_cell())
        }
        None => Ok(nlog_error_val(span, format!("unknown log level '{level_str}'"))),
    }
}

fn nlog_context(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            codes::E2640_NLOG_ARITY,
            "nlog_context() expects 1 argument (an object)",
        ));
    }
    let map = match &*args[0].borrow() {
        Value::Object(map) => map.clone(),
        other => {
            return Err(type_err(
                span,
                format!("nlog_context() expects an object, got {}", other.type_name()),
            ))
        }
    };
    let mut st = state().lock().map_err(|_| {
        RuntimeError::at(span, codes::E2641_NLOG_ERROR, "nlog state lock poisoned")
    })?;
    let mut pairs: Vec<(String, FieldValue)> = map
        .iter()
        .map(|(k, v)| (k.clone(), FieldValue::from_value(&v.borrow())))
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, v) in pairs {
        if let Some(existing) = st.context.iter_mut().find(|(ek, _)| *ek == k) {
            existing.1 = v;
        } else {
            st.context.push((k, v));
        }
    }
    Ok(Value::Nil.ref_cell())
}

fn nlog_clear_context(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if !args.is_empty() {
        return Err(RuntimeError::at(
            span,
            codes::E2640_NLOG_ARITY,
            "nlog_clear_context() expects 0 arguments",
        ));
    }
    let mut st = state().lock().map_err(|_| {
        RuntimeError::at(span, codes::E2641_NLOG_ERROR, "nlog state lock poisoned")
    })?;
    st.context.clear();
    Ok(Value::Nil.ref_cell())
}

fn nlog_trace(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    log_at(LEVEL_TRACE, "nlog_trace", args, span)
}

fn nlog_debug(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    log_at(LEVEL_DEBUG, "nlog_debug", args, span)
}

fn nlog_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    log_at(LEVEL_INFO, "nlog_info", args, span)
}

fn nlog_warn(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    log_at(LEVEL_WARN, "nlog_warn", args, span)
}

fn nlog_error_fn(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    log_at(LEVEL_ERROR, "nlog_error", args, span)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nlog_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nlog_fns![
    ("nlog_init", "init", nlog_init),
    ("nlog_set_level", "set_level", nlog_set_level),
    ("nlog_get_level", "get_level", nlog_get_level),
    ("nlog_enabled", "enabled", nlog_enabled),
    ("nlog_context", "context", nlog_context),
    ("nlog_clear_context", "clear_context", nlog_clear_context),
    ("nlog_trace", "trace", nlog_trace),
    ("nlog_debug", "debug", nlog_debug),
    ("nlog_info", "info", nlog_info),
    ("nlog_warn", "warn", nlog_warn),
    ("nlog_error", "error", nlog_error_fn),
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

pub const MODULE_NAME: &str = "nlog";
pub const MODULE_PATHS: &[&str] = &["nlog", "std/nlog"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn level_parsing() {
        assert_eq!(level_from_str("INFO"), Some(LEVEL_INFO));
        assert_eq!(level_from_str("warning"), Some(LEVEL_WARN));
        assert_eq!(level_from_str("bogus"), None);
    }

    #[test]
    fn json_escaping() {
        assert_eq!(json_escape("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    }

    #[test]
    fn field_value_rendering() {
        assert_eq!(FieldValue::Int(42).json(), "42");
        assert_eq!(FieldValue::Str("hi there".into()).json(), "\"hi there\"");
        assert_eq!(FieldValue::Bool(true).text(), "true");
    }
}
