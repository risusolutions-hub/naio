//! Native ncron standard library — standard 5-field cron expressions (minute,
//! hour, day-of-month, month, day-of-week). Hand-rolled parser; pure functions
//! only (no background scheduler).
//!
//! Import with `import "ncron"` (or `import "std/ncron"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_time::{
    civil_from_days, civil_to_ms, days_from_civil, ms_to_civil, weekday_from_days, CivilDateTime,
    Timezone,
};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

// Wired in codes.rs by central integration.
const E2910_NCRON_ARITY: u32 = 2910;
const E2911_NCRON_ERROR: u32 = 2911;
const E2912_NCRON_PARSE: u32 = 2912;

const MAX_SEARCH_MINUTES: i64 = 366 * 24 * 60;

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
            E2910_NCRON_ARITY,
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
            E2910_NCRON_ARITY,
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

fn parse_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2912_NCRON_PARSE, "ncron_error", msg.into(), span)
}

fn cron_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2911_NCRON_ERROR, "ncron_error", msg.into(), span)
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn system_now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Cron field model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct FieldMatcher {
    any: bool,
    min: u32,
    max: u32,
    values: Vec<bool>,
}

impl FieldMatcher {
    fn all(min: u32, max: u32) -> Self {
        let size = (max - min + 1) as usize;
        Self {
            any: true,
            min,
            max,
            values: vec![true; size],
        }
    }

    fn from_values(min: u32, max: u32, picks: &[u32], any: bool) -> Result<Self, String> {
        let size = (max - min + 1) as usize;
        let mut values = vec![false; size];
        for &v in picks {
            if v < min || v > max {
                return Err(format!("value {v} out of range {min}-{max}"));
            }
            values[(v - min) as usize] = true;
        }
        Ok(Self {
            any,
            min,
            max,
            values,
        })
    }

    fn matches(&self, v: u32) -> bool {
        if v < self.min || v > self.max {
            return false;
        }
        self.values[(v - self.min) as usize]
    }
}

#[derive(Clone, Debug)]
struct CronExpr {
    minute: FieldMatcher,
    hour: FieldMatcher,
    dom: FieldMatcher,
    month: FieldMatcher,
    dow: FieldMatcher,
    raw: [String; 5],
}

fn normalize_dow(v: u32) -> u32 {
    if v == 7 {
        0
    } else {
        v
    }
}

fn parse_number(token: &str) -> Result<u32, String> {
    if token.is_empty() {
        return Err("empty number".into());
    }
    token
        .parse::<u32>()
        .map_err(|_| format!("invalid number '{token}'"))
}

fn expand_part(
    part: &str,
    min: u32,
    max: u32,
    normalize: fn(u32) -> u32,
) -> Result<Vec<u32>, String> {
    let (base, step) = if let Some((b, s)) = part.split_once('/') {
        let step = parse_number(s)?;
        if step == 0 {
            return Err("step must be >= 1".into());
        }
        (b, step)
    } else {
        (part, 1)
    };

    let (start, end) = if base == "*" {
        (min, max)
    } else if let Some((lo, hi)) = base.split_once('-') {
        let start = normalize(parse_number(lo)?);
        let end = normalize(parse_number(hi)?);
        if start > end {
            return Err(format!("invalid range '{base}'"));
        }
        (start, end)
    } else {
        let v = normalize(parse_number(base)?);
        (v, v)
    };

    let mut out = Vec::new();
    let mut cur = start;
    while cur <= end {
        if cur >= min && cur <= max {
            out.push(cur);
        }
        if cur == end {
            break;
        }
        cur = cur.saturating_add(step);
        if cur <= start && step > 0 {
            break;
        }
    }
    if out.is_empty() {
        return Err(format!("no values in field part '{part}'"));
    }
    Ok(out)
}

fn parse_field(
    field: &str,
    min: u32,
    max: u32,
    normalize: fn(u32) -> u32,
) -> Result<FieldMatcher, String> {
    if field.trim().is_empty() {
        return Err("empty field".into());
    }
    if field == "*" {
        return Ok(FieldMatcher::all(min, max));
    }
    let mut picks = Vec::new();
    for part in field.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err("empty list entry".into());
        }
        picks.extend(expand_part(part, min, max, normalize)?);
    }
    picks.sort_unstable();
    picks.dedup();
    Ok(FieldMatcher::from_values(min, max, &picks, false)?)
}

fn split_expr(expr: &str) -> Result<[String; 5], String> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return Err(format!("expected 5 fields, got {}", parts.len()));
    }
    Ok([
        parts[0].to_string(),
        parts[1].to_string(),
        parts[2].to_string(),
        parts[3].to_string(),
        parts[4].to_string(),
    ])
}

fn parse_cron(expr: &str) -> Result<CronExpr, String> {
    let raw = split_expr(expr)?;
    Ok(CronExpr {
        minute: parse_field(&raw[0], 0, 59, |v| v)?,
        hour: parse_field(&raw[1], 0, 23, |v| v)?,
        dom: parse_field(&raw[2], 1, 31, |v| v)?,
        month: parse_field(&raw[3], 1, 12, |v| v)?,
        dow: parse_field(&raw[4], 0, 7, normalize_dow)?,
        raw,
    })
}

fn niao_weekday_to_cron(weekday: usize) -> u32 {
    // niao_time: 0=Mon..6=Sun; cron: 0=Sun, 1=Mon, .. 6=Sat
    ((weekday as u32) + 1) % 7
}

fn cron_dow_for(year: i32, month: u32, day: u32) -> u32 {
    let z = days_from_civil(year, month, day);
    niao_weekday_to_cron(weekday_from_days(z))
}

fn day_matches(cron: &CronExpr, year: i32, month: u32, day: u32) -> bool {
    let dom_match = cron.dom.matches(day);
    let dow_match = cron.dow.matches(cron_dow_for(year, month, day));
    if cron.dom.any && cron.dow.any {
        true
    } else if cron.dom.any {
        dow_match
    } else if cron.dow.any {
        dom_match
    } else {
        dom_match || dow_match
    }
}

fn matches_civil(cron: &CronExpr, civil: &CivilDateTime) -> bool {
    if !cron.month.matches(civil.month) {
        return false;
    }
    if !day_matches(cron, civil.year, civil.month, civil.day) {
        return false;
    }
    if !cron.hour.matches(civil.hour) {
        return false;
    }
    cron.minute.matches(civil.minute)
}

fn advance_minute(civil: &mut CivilDateTime) {
    civil.second = 0;
    civil.millisecond = 0;
    civil.minute += 1;
    if civil.minute < 60 {
        return;
    }
    civil.minute = 0;
    civil.hour += 1;
    if civil.hour < 24 {
        return;
    }
    civil.hour = 0;
    let z = days_from_civil(civil.year, civil.month, civil.day) + 1;
    let (y, m, d) = civil_from_days(z);
    civil.year = y;
    civil.month = m;
    civil.day = d;
}

fn align_to_minute(from_ms: i64, tz: &Timezone) -> Result<CivilDateTime, String> {
    let mut civil = ms_to_civil(from_ms, tz)?;
    civil.second = 0;
    civil.millisecond = 0;
    let aligned = civil_to_ms(&civil, tz)?;
    if aligned < from_ms {
        advance_minute(&mut civil);
    }
    Ok(civil)
}

fn next_after(cron: &CronExpr, from_ms: i64, tz: &Timezone) -> Result<i64, String> {
    let mut civil = align_to_minute(from_ms, tz)?;
    for _ in 0..MAX_SEARCH_MINUTES {
        if matches_civil(cron, &civil) {
            return civil_to_ms(&civil, tz);
        }
        advance_minute(&mut civil);
    }
    Err("no matching occurrence within one year".into())
}

fn matches_at(expr: &str, unix_ms: i64, tz: &Timezone) -> Result<bool, String> {
    let cron = parse_cron(expr)?;
    let civil = ms_to_civil(unix_ms, tz)?;
    Ok(matches_civil(&cron, &civil))
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ncron_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncron_valid", span)?;
    let expr = string_arg(args, 0, "ncron_valid", span)?;
    bool_val(parse_cron(&expr).is_ok())
}

fn ncron_next(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncron_next", span)?;
    let expr = string_arg(args, 0, "ncron_next", span)?;
    let cron = match parse_cron(&expr) {
        Ok(c) => c,
        Err(msg) => return Ok(parse_err(span, msg)),
    };
    let from_ms = if args.len() == 2 {
        int_arg(args, 1, "ncron_next", span)?
    } else {
        system_now_unix_ms()
    };
    let tz = Timezone::local();
    match next_after(&cron, from_ms, &tz) {
        Ok(ms) => Ok(Value::Int(ms).ref_cell()),
        Err(msg) => Ok(cron_err(span, msg)),
    }
}

fn ncron_fields(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncron_fields", span)?;
    let expr = string_arg(args, 0, "ncron_fields", span)?;
    match parse_cron(&expr) {
        Ok(cron) => {
            let mut out = HashMap::new();
            out.insert(
                "minute".to_string(),
                Value::String(cron.raw[0].clone()).ref_cell(),
            );
            out.insert(
                "hour".to_string(),
                Value::String(cron.raw[1].clone()).ref_cell(),
            );
            out.insert(
                "day".to_string(),
                Value::String(cron.raw[2].clone()).ref_cell(),
            );
            out.insert(
                "month".to_string(),
                Value::String(cron.raw[3].clone()).ref_cell(),
            );
            out.insert(
                "weekday".to_string(),
                Value::String(cron.raw[4].clone()).ref_cell(),
            );
            Ok(Value::Object(out).ref_cell())
        }
        Err(msg) => Ok(parse_err(span, msg)),
    }
}

fn ncron_match(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncron_match", span)?;
    let expr = string_arg(args, 0, "ncron_match", span)?;
    let unix_ms = int_arg(args, 1, "ncron_match", span)?;
    let tz = Timezone::local();
    match matches_at(&expr, unix_ms, &tz) {
        Ok(b) => bool_val(b),
        Err(msg) => Ok(parse_err(span, msg)),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncron_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncron_fns![
    ("ncron_valid", "valid", ncron_valid),
    ("ncron_next", "next", ncron_next),
    ("ncron_fields", "fields", ncron_fields),
    ("ncron_match", "match", ncron_match),
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

pub const MODULE_NAME: &str = "ncron";
pub const MODULE_PATHS: &[&str] = &["ncron", "std/ncron"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_every_five_minutes() {
        let cron = parse_cron("*/5 * * * *").unwrap();
        assert!(cron.minute.matches(0));
        assert!(cron.minute.matches(5));
        assert!(cron.minute.matches(10));
        assert!(cron.minute.matches(55));
        assert!(!cron.minute.matches(3));
        assert!(!cron.minute.matches(59));
        assert!(cron.hour.any);
        assert!(cron.dom.any);
    }

    #[test]
    fn rejects_invalid_expressions() {
        assert!(parse_cron("* * *").is_err());
        assert!(parse_cron("60 * * * *").is_err());
        assert!(parse_cron("* 24 * * *").is_err());
        assert!(parse_cron("* * 0 * *").is_err());
        assert!(parse_cron("* * * 13 *").is_err());
        assert!(parse_cron("* * * * 8").is_err());
        assert!(parse_cron("*/0 * * * *").is_err());
    }

    #[test]
    fn lists_ranges_and_steps() {
        let cron = parse_cron("1,15,30-45/15 0 1,15 1,6,12 1-5").unwrap();
        assert!(cron.minute.matches(1));
        assert!(cron.minute.matches(15));
        assert!(cron.minute.matches(30));
        assert!(cron.minute.matches(45));
        assert!(!cron.minute.matches(0));
        assert!(cron.dom.matches(1));
        assert!(cron.dom.matches(15));
        assert!(!cron.dom.matches(2));
        assert!(cron.month.matches(1));
        assert!(cron.month.matches(6));
        assert!(cron.dow.matches(1));
        assert!(cron.dow.matches(5));
        assert!(cron.dow.matches(0)); // 7 normalizes to 0
        let cron7 = parse_cron("* * * * 7").unwrap();
        assert!(cron7.dow.matches(0));
    }

    #[test]
    fn next_occurrence_every_five_minutes() {
        let tz = Timezone::utc();
        let from = civil_to_ms(
            &CivilDateTime {
                year: 2026,
                month: 7,
                day: 12,
                hour: 10,
                minute: 3,
                second: 0,
                millisecond: 0,
            },
            &tz,
        )
        .unwrap();
        let cron = parse_cron("*/5 * * * *").unwrap();
        let next = next_after(&cron, from, &tz).unwrap();
        let expected = civil_to_ms(
            &CivilDateTime {
                year: 2026,
                month: 7,
                day: 12,
                hour: 10,
                minute: 5,
                second: 0,
                millisecond: 0,
            },
            &tz,
        )
        .unwrap();
        assert_eq!(next, expected);
    }

    #[test]
    fn next_from_exact_match_returns_same_minute() {
        let tz = Timezone::utc();
        let from = civil_to_ms(
            &CivilDateTime {
                year: 2026,
                month: 7,
                day: 12,
                hour: 10,
                minute: 5,
                second: 0,
                millisecond: 0,
            },
            &tz,
        )
        .unwrap();
        let cron = parse_cron("*/5 * * * *").unwrap();
        let next = next_after(&cron, from, &tz).unwrap();
        assert_eq!(next, from);
    }

    #[test]
    fn match_and_dom_dow_or_logic() {
        let tz = Timezone::utc();
        // 2026-07-12 is Sunday (cron dow 0)
        let ms = civil_to_ms(
            &CivilDateTime {
                year: 2026,
                month: 7,
                day: 12,
                hour: 12,
                minute: 0,
                second: 0,
                millisecond: 0,
            },
            &tz,
        )
        .unwrap();
        assert!(matches_at("0 12 12 7 0", ms, &tz).unwrap());
        assert!(matches_at("0 12 12 7 1", ms, &tz).unwrap()); // dom matches
        assert!(matches_at("0 12 13 7 0", ms, &tz).unwrap()); // dow matches
        assert!(!matches_at("0 12 13 7 1", ms, &tz).unwrap());
    }
}
