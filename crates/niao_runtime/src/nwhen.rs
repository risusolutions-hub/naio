//! Native nwhen standard library — natural-language + fuzzy date parsing
//! (~dateparser, dateutil subset; extends `time`).
//!
//! Import with `import "nwhen"` (or `import "std/nwhen"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_time::{civil_to_ms, ms_to_civil, weekday_from_days, days_from_civil, WEEKDAY_NAMES, Timezone};
use niao_when::{
    batch_parse, parse, parse_many, search, supported_languages, valid, DateOrder, ParseOptions,
    PreferDirection, RequireParts, WhenError,
};
use std::collections::HashMap;
use std::rc::Rc;

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4362_NWHEN_TYPE, msg.into())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4360_NWHEN_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nwhen_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4361_NWHEN_ERROR, "nwhen_error", msg.into(), span)
}

fn map_when_err(span: Span, err: WhenError) -> ValueRef {
    let code = match &err {
        WhenError::Empty | WhenError::NoDate | WhenError::Ambiguous(_) => codes::E4361_NWHEN_ERROR,
        WhenError::InvalidDate(_) | WhenError::InvalidTime(_) => codes::E4363_NWHEN_PARSE,
        WhenError::Unsupported(_) => codes::E4361_NWHEN_ERROR,
    };
    error_value(code, "nwhen_error", err.message(), span)
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

fn int_field_opt(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<i64> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => Some(n),
        _ => None,
    }
}

fn string_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<String> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    }
}

fn string_array_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<Vec<String>> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Array(items)) => {
            let mut out = Vec::new();
            for item in items {
                if let Value::String(s) = &*item.borrow() {
                    out.push(s.clone());
                }
            }
            if out.is_empty() { None } else { Some(out) }
        }
        _ => None,
    }
}

fn parse_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> ParseOptions {
    let mut opts = ParseOptions::default();
    if let Some(ms) = int_field_opt(map, "base_ms").or_else(|| int_field_opt(map, "base")) {
        opts.base_ms = ms;
    }
    if let Some(tz) = string_field(map, "timezone").or_else(|| string_field(map, "tz")) {
        opts.timezone = tz;
    }
    if let Some(p) = string_field(map, "prefer") {
        opts.prefer = match p.to_ascii_lowercase().as_str() {
            "past" => PreferDirection::Past,
            "current" | "current_period" => PreferDirection::Current,
            _ => PreferDirection::Future,
        };
    }
    if let Some(d) = string_field(map, "date_order") {
        opts.date_order = match d.to_ascii_lowercase().as_str() {
            "dmy" => DateOrder::Dmy,
            "ymd" => DateOrder::Ymd,
            _ => DateOrder::Mdy,
        };
    }
    if map.is_some() {
        opts.fuzzy = bool_field(map, "fuzzy", opts.fuzzy);
    }
    if let Some(r) = string_field(map, "require") {
        opts.require = match r.to_ascii_lowercase().as_str() {
            "date" => RequireParts::Date,
            "time" => RequireParts::Time,
            "both" => RequireParts::Both,
            _ => RequireParts::Any,
        };
    }
    if let Some(langs) = string_array_field(map, "languages") {
        opts.languages = langs;
    }
    opts
}

fn datetime_object(ms: i64, tz: &Timezone, extra: Option<(&str, Value)>) -> Result<Value, String> {
    let civil = ms_to_civil(ms, tz)?;
    let wd = weekday_from_days(days_from_civil(civil.year, civil.month, civil.day));
    let offset_ms = tz.offset_at_ms(ms)? as i64 * 1000;
    let mut map = HashMap::new();
    let insert = |map: &mut HashMap<String, ValueRef>, k: &str, v: Value| {
        map.insert(k.to_string(), v.ref_cell());
    };
    insert(&mut map, "year", Value::Int(civil.year as i64));
    insert(&mut map, "month", Value::Int(civil.month as i64));
    insert(&mut map, "day", Value::Int(civil.day as i64));
    insert(&mut map, "hour", Value::Int(civil.hour as i64));
    insert(&mut map, "minute", Value::Int(civil.minute as i64));
    insert(&mut map, "second", Value::Int(civil.second as i64));
    insert(&mut map, "millisecond", Value::Int(civil.millisecond as i64));
    insert(&mut map, "weekday", Value::Int(wd as i64));
    insert(
        &mut map,
        "weekday_name",
        Value::String(WEEKDAY_NAMES[wd].to_string()),
    );
    insert(&mut map, "unix_ms", Value::Int(ms));
    insert(&mut map, "timezone", Value::String(tz.name().to_string()));
    insert(&mut map, "utc_offset_ms", Value::Int(offset_ms));
    if let Some((k, v)) = extra {
        insert(&mut map, k, v);
    }
    Ok(Value::Object(map))
}

fn parsed_to_value(parsed: &niao_when::ParsedDate, opts: &ParseOptions, span: Span) -> ValueRef {
    match opts.resolve_tz() {
        Ok(tz) => match datetime_object(
            parsed.unix_ms,
            &tz,
            Some((
                "matched",
                Value::String(parsed.matched.clone()),
            )),
        ) {
            Ok(obj) => {
                let mut m = match obj {
                    Value::Object(map) => map,
                    _ => return nwhen_err(span, "internal error"),
                };
                m.insert("has_date".into(), Value::Bool(parsed.has_date).ref_cell());
                m.insert("has_time".into(), Value::Bool(parsed.has_time).ref_cell());
                Value::Object(m).ref_cell()
            }
            Err(msg) => nwhen_err(span, msg),
        },
        Err(msg) => nwhen_err(span, msg),
    }
}

/// nwhen.parse(text, options?) — parse natural-language date/time.
fn nwhen_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nwhen_parse", span)?;
    let text = string_arg(args, 0, "nwhen_parse", span)?;
    let opts = parse_opts_from_map(optional_object_arg(args, 1));
    match parse(&text, &opts) {
        Ok(p) => Ok(parsed_to_value(&p, &opts, span)),
        Err(e) => Ok(map_when_err(span, e)),
    }
}

/// nwhen.valid(text, options?) — true when text parses.
fn nwhen_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nwhen_valid", span)?;
    let text = string_arg(args, 0, "nwhen_valid", span)?;
    let opts = parse_opts_from_map(optional_object_arg(args, 1));
    Ok(Value::Bool(valid(&text, &opts)).ref_cell())
}

/// nwhen.parse_many(text, options?) — ranked parse candidates.
fn nwhen_parse_many(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nwhen_parse_many", span)?;
    let text = string_arg(args, 0, "nwhen_parse_many", span)?;
    let opts = parse_opts_from_map(optional_object_arg(args, 1));
    match parse_many(&text, &opts) {
        Ok(list) => {
            let arr: Vec<ValueRef> = list
                .iter()
                .map(|p| parsed_to_value(p, &opts, span))
                .collect();
            Ok(Value::Array(arr).ref_cell())
        }
        Err(e) => Ok(map_when_err(span, e)),
    }
}

/// nwhen.search(text, options?) — find date substrings in text.
fn nwhen_search(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nwhen_search", span)?;
    let text = string_arg(args, 0, "nwhen_search", span)?;
    let opts = parse_opts_from_map(optional_object_arg(args, 1));
    match search(&text, &opts) {
        Ok(hits) => {
            let tz = match opts.resolve_tz() {
                Ok(t) => t,
                Err(msg) => return Ok(nwhen_err(span, msg)),
            };
            let mut arr = Vec::with_capacity(hits.len());
            for h in hits {
                let mut entry = HashMap::new();
                entry.insert("text".into(), Value::String(h.text).ref_cell());
                entry.insert("start".into(), Value::Int(h.start as i64).ref_cell());
                entry.insert("end".into(), Value::Int(h.end as i64).ref_cell());
                match datetime_object(h.unix_ms, &tz, None) {
                    Ok(Value::Object(dt)) => {
                        entry.insert("date".into(), Value::Object(dt).ref_cell());
                    }
                    _ => {}
                }
                arr.push(Value::Object(entry).ref_cell());
            }
            Ok(Value::Array(arr).ref_cell())
        }
        Err(e) => Ok(map_when_err(span, e)),
    }
}

/// nwhen.batch(texts, options?, threads?) — parallel parse.
fn nwhen_batch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nwhen_batch", span)?;
    let texts_v = match &*args[0].borrow() {
        Value::Array(items) => items.clone(),
        other => {
            return Err(type_err(
                span,
                format!("nwhen_batch() expects array of strings, got {}", other.type_name()),
            ));
        }
    };
    let mut texts = Vec::with_capacity(texts_v.len());
    for (i, v) in texts_v.iter().enumerate() {
        match &*v.borrow() {
            Value::String(s) => texts.push(s.clone()),
            other => {
                return Err(type_err(
                    span,
                    format!("batch item {} must be string, got {}", i + 1, other.type_name()),
                ));
            }
        }
    }
    let opts = parse_opts_from_map(optional_object_arg(args, 1));
    let threads = if args.len() >= 3 {
        match &*args[2].borrow() {
            Value::Int(n) if *n >= 0 => *n as usize,
            other => {
                return Err(type_err(
                    span,
                    format!("threads must be non-negative int, got {}", other.type_name()),
                ));
            }
        }
    } else {
        0
    };
    let results = batch_parse(&texts, &opts, threads);
    let mut arr = Vec::with_capacity(results.len());
    for r in results {
        arr.push(match r {
            Ok(p) => parsed_to_value(&p, &opts, span),
            Err(e) => map_when_err(span, e),
        });
    }
    Ok(Value::Array(arr).ref_cell())
}

/// nwhen.languages() — supported language tags.
fn nwhen_languages(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let langs: Vec<ValueRef> = supported_languages()
        .iter()
        .map(|s| Value::String((*s).to_string()).ref_cell())
        .collect();
    Ok(Value::Array(langs).ref_cell())
}

/// nwhen.to_unix_ms(date_obj, timezone?) — extract unix ms from a datetime object.
fn nwhen_to_unix_ms(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nwhen_to_unix_ms", span)?;
    match &*args[0].borrow() {
        Value::Object(m) => {
            if let Some(v) = m.get("unix_ms") {
                if let Value::Int(n) = &*v.borrow() {
                    return Ok(Value::Int(*n).ref_cell());
                }
            }
            if let (Some(y), Some(mo), Some(d)) = (
                m.get("year").map(|v| v.borrow().clone()),
                m.get("month").map(|v| v.borrow().clone()),
                m.get("day").map(|v| v.borrow().clone()),
            ) {
                let tz_name = if args.len() == 2 {
                    string_arg(args, 1, "nwhen_to_unix_ms", span)?
                } else {
                    m.get("timezone")
                        .and_then(|v| match &*v.borrow() {
                            Value::String(s) => Some(s.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| "UTC".into())
                };
                let tz = Timezone::named(&tz_name).map_err(|e| type_err(span, e))?;
                let civil = niao_time::CivilDateTime {
                    year: match y { Value::Int(n) => n as i32, _ => return Ok(nwhen_err(span, "invalid year")) },
                    month: match mo { Value::Int(n) => n as u32, _ => return Ok(nwhen_err(span, "invalid month")) },
                    day: match d { Value::Int(n) => n as u32, _ => return Ok(nwhen_err(span, "invalid day")) },
                    hour: m.get("hour").and_then(|v| match &*v.borrow() { Value::Int(n) => Some(*n as u32), _ => None }).unwrap_or(0),
                    minute: m.get("minute").and_then(|v| match &*v.borrow() { Value::Int(n) => Some(*n as u32), _ => None }).unwrap_or(0),
                    second: m.get("second").and_then(|v| match &*v.borrow() { Value::Int(n) => Some(*n as u32), _ => None }).unwrap_or(0),
                    millisecond: m.get("millisecond").and_then(|v| match &*v.borrow() { Value::Int(n) => Some(*n as u32), _ => None }).unwrap_or(0),
                };
                match civil_to_ms(&civil, &tz) {
                    Ok(ms) => Ok(Value::Int(ms).ref_cell()),
                    Err(msg) => Ok(nwhen_err(span, msg)),
                }
            } else {
                Ok(nwhen_err(span, "datetime object missing fields"))
            }
        }
        other => Err(type_err(
            span,
            format!("nwhen_to_unix_ms expects datetime object, got {}", other.type_name()),
        )),
    }
}

macro_rules! nwhen_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nwhen_fns![
    ("nwhen_parse", "parse", nwhen_parse),
    ("nwhen_valid", "valid", nwhen_valid),
    ("nwhen_parse_many", "parse_many", nwhen_parse_many),
    ("nwhen_search", "search", nwhen_search),
    ("nwhen_batch", "batch", nwhen_batch),
    ("nwhen_languages", "languages", nwhen_languages),
    ("nwhen_to_unix_ms", "to_unix_ms", nwhen_to_unix_ms),
];

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nwhen";
pub const MODULE_PATHS: &[&str] = &["nwhen", "std/nwhen"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn parse_tomorrow() {
        let out = nwhen_parse(
            &[Value::String("tomorrow at noon".into()).ref_cell()],
            span(),
        )
        .unwrap();
        match &*out.borrow() {
            Value::Object(m) => assert!(m.contains_key("unix_ms")),
            _ => panic!("expected object"),
        }
    }
}
