//! Native time standard library — wall clock, formatting, parsing, time zones,
//! and date arithmetic via `niao_time`.
//!
//! Registered as prefixed builtins (`time_now_unix_ms`, `time_format`, ...).
//! Import with `import "time"` (or `import "std/time"`) for the namespace API.

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_time::{
    days_from_civil, days_in_month, format_datetime, is_leap_year, is_valid_date, list_timezones,
    parse_datetime, to_rfc3339, weekday_from_days, civil_to_ms, ms_to_civil, CivilDateTime,
    DateTime, Timezone, WEEKDAY_NAMES,
};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static PROCESS_START: OnceLock<Instant> = OnceLock::new();

fn perf_now_us() -> i64 {
    PROCESS_START
        .get_or_init(Instant::now)
        .elapsed()
        .as_micros() as i64
}

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
            codes::E1600_TIME_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E1600_TIME_ARITY,
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

fn time_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E1601_TIME_ERROR, "time_error", msg.into(), span)
}

fn ok_nil() -> ValueRef {
    Value::Nil.ref_cell()
}

fn ok_int(n: i64) -> ValueRef {
    Value::Int(n).ref_cell()
}

fn ok_float(f: f64) -> ValueRef {
    Value::Float(f).ref_cell()
}

fn ok_bool(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
}

fn ok_string(s: impl Into<String>) -> ValueRef {
    Value::String(s.into()).ref_cell()
}

fn system_now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn resolve_tz(name: &str) -> Result<Timezone, String> {
    Timezone::named(name)
}

struct DateTimeFields {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millisecond: u32,
    weekday: usize,
    timezone: String,
}

fn extract_fields(civil: &CivilDateTime, timezone: String) -> DateTimeFields {
    let wd = weekday_from_days(days_from_civil(civil.year, civil.month, civil.day));
    DateTimeFields {
        year: civil.year,
        month: civil.month,
        day: civil.day,
        hour: civil.hour,
        minute: civil.minute,
        second: civil.second,
        millisecond: civil.millisecond,
        weekday: wd,
        timezone,
    }
}

fn view_in_tz(ms: i64, tz: &Timezone) -> Result<DateTimeFields, String> {
    let civil = ms_to_civil(ms, tz)?;
    Ok(extract_fields(&civil, tz.name().to_string()))
}

fn datetime_object(fields: &DateTimeFields, unix_ms: i64, utc_offset_ms: i64) -> Value {
    let weekday = fields.weekday as i64;
    let mut map = HashMap::new();
    let insert = |map: &mut HashMap<String, ValueRef>, k: &str, v: Value| {
        map.insert(k.to_string(), v.ref_cell());
    };
    insert(&mut map, "year", Value::Int(fields.year as i64));
    insert(&mut map, "month", Value::Int(fields.month as i64));
    insert(&mut map, "day", Value::Int(fields.day as i64));
    insert(&mut map, "hour", Value::Int(fields.hour as i64));
    insert(&mut map, "minute", Value::Int(fields.minute as i64));
    insert(&mut map, "second", Value::Int(fields.second as i64));
    insert(&mut map, "millisecond", Value::Int(fields.millisecond as i64));
    insert(&mut map, "weekday", Value::Int(weekday));
    insert(
        &mut map,
        "weekday_name",
        Value::String(WEEKDAY_NAMES[fields.weekday].to_string()),
    );
    insert(&mut map, "unix_ms", Value::Int(unix_ms));
    insert(
        &mut map,
        "timezone",
        Value::String(fields.timezone.clone()),
    );
    insert(&mut map, "utc_offset_ms", Value::Int(utc_offset_ms));
    Value::Object(map)
}

fn make_datetime_object(ms: i64, tz: &Timezone) -> Result<Value, String> {
    let fields = view_in_tz(ms, tz)?;
    let offset_ms = tz.offset_at_ms(ms)? as i64 * 1000;
    Ok(datetime_object(&fields, ms, offset_ms))
}

fn parse_with_tz(text: &str, fmt: &str, tz: &Timezone) -> Result<i64, String> {
    let civil = parse_datetime(text, fmt)?;
    civil_to_ms(&civil, tz)
}

fn format_with_tz(ms: i64, fmt: &str, tz: &Timezone) -> Result<String, String> {
    let civil = ms_to_civil(ms, tz)?;
    let offset = tz.offset_at_ms(ms)?;
    format_datetime(&civil, fmt, offset)
}

fn time_now_unix_ms(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(ok_int(system_now_unix_ms()))
}

fn time_now_perf_us(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(ok_int(perf_now_us()))
}

fn time_elapsed_ms(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "time_elapsed_ms", span)?;
    let t0 = match &*args[0].borrow() {
        Value::Int(n) => *n,
        _ => {
            return Err(type_err(
                span,
                "time_elapsed_ms expects microsecond start timestamp from now_perf_us",
            ));
        }
    };
    let us = perf_now_us().saturating_sub(t0);
    Ok(ok_int((us + 500) / 1000))
}

fn time_elapsed_us(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "time_elapsed_us", span)?;
    let t0 = match &*args[0].borrow() {
        Value::Int(n) => *n,
        _ => {
            return Err(type_err(
                span,
                "time_elapsed_us expects microsecond start timestamp from now_perf_us",
            ));
        }
    };
    Ok(ok_int(perf_now_us().saturating_sub(t0)))
}

fn time_elapsed_bench_ms(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "time_elapsed_bench_ms", span)?;
    let t0 = match &*args[0].borrow() {
        Value::Int(n) => *n,
        _ => {
            return Err(type_err(
                span,
                "time_elapsed_bench_ms expects microsecond start timestamp from now_perf_us",
            ));
        }
    };
    let us = perf_now_us().saturating_sub(t0);
    Ok(ok_float(us as f64 / 1000.0))
}

fn time_now_unix_s(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(ok_int(system_now_unix_ms() / 1000))
}

fn time_now_iso(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let ms = system_now_unix_ms();
    Ok(ok_string(
        to_rfc3339(ms, &Timezone::utc()).unwrap_or_else(|_| "1970-01-01T00:00:00.000Z".into()),
    ))
}

fn time_now_iso_local(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let ms = system_now_unix_ms();
    Ok(ok_string(
        to_rfc3339(ms, &Timezone::local()).unwrap_or_else(|_| "1970-01-01T00:00:00.000Z".into()),
    ))
}

fn time_now(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "time_now", span)?;
    let ms = system_now_unix_ms();
    let tz = if args.len() == 1 {
        resolve_tz(&string_arg(args, 0, "time_now", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::local()
    };
    match make_datetime_object(ms, &tz) {
        Ok(obj) => Ok(obj.ref_cell()),
        Err(msg) => Ok(time_error(span, msg)),
    }
}

fn time_format(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "time_format", span)?;
    let ms = int_arg(args, 0, "time_format", span)?;
    let fmt = string_arg(args, 1, "time_format", span)?;
    let tz = if args.len() == 3 {
        resolve_tz(&string_arg(args, 2, "time_format", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::utc()
    };
    match format_with_tz(ms, &fmt, &tz) {
        Ok(s) => Ok(ok_string(s)),
        Err(msg) => Ok(time_error(span, msg)),
    }
}

fn time_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "time_parse", span)?;
    let text = string_arg(args, 0, "time_parse", span)?;
    let fmt = string_arg(args, 1, "time_parse", span)?;
    let tz = if args.len() == 3 {
        resolve_tz(&string_arg(args, 2, "time_parse", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::utc()
    };
    match parse_with_tz(&text, &fmt, &tz) {
        Ok(ms) => Ok(ok_int(ms)),
        Err(msg) => Ok(time_error(span, msg)),
    }
}

fn time_to_iso(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "time_to_iso", span)?;
    let ms = int_arg(args, 0, "time_to_iso", span)?;
    let tz = if args.len() == 2 {
        resolve_tz(&string_arg(args, 1, "time_to_iso", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::utc()
    };
    match to_rfc3339(ms, &tz) {
        Ok(s) => Ok(ok_string(s)),
        Err(msg) => Ok(time_error(span, msg)),
    }
}

fn time_decompose(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "time_decompose", span)?;
    let ms = int_arg(args, 0, "time_decompose", span)?;
    let tz = if args.len() == 2 {
        resolve_tz(&string_arg(args, 1, "time_decompose", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::local()
    };
    match make_datetime_object(ms, &tz) {
        Ok(obj) => Ok(obj.ref_cell()),
        Err(msg) => Ok(time_error(span, msg)),
    }
}

fn time_from_parts(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 8, "time_from_parts", span)?;
    let year = int_arg(args, 0, "time_from_parts", span)? as i32;
    let month = int_arg(args, 1, "time_from_parts", span)? as u32;
    let day = int_arg(args, 2, "time_from_parts", span)? as u32;
    let hour = if args.len() > 3 {
        int_arg(args, 3, "time_from_parts", span)? as u32
    } else {
        0
    };
    let minute = if args.len() > 4 {
        int_arg(args, 4, "time_from_parts", span)? as u32
    } else {
        0
    };
    let second = if args.len() > 5 {
        int_arg(args, 5, "time_from_parts", span)? as u32
    } else {
        0
    };
    let millisecond = if args.len() > 6 {
        int_arg(args, 6, "time_from_parts", span)? as u32
    } else {
        0
    };
    let tz = if args.len() > 7 {
        resolve_tz(&string_arg(args, 7, "time_from_parts", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::local()
    };

    if !is_valid_date(year, month, day) {
        return Err(type_err(span, "invalid year/month/day"));
    }
    let civil = CivilDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        millisecond,
    };
    match civil_to_ms(&civil, &tz) {
        Ok(ms) => Ok(ok_int(ms)),
        Err(msg) => Ok(time_error(span, msg)),
    }
}

fn time_add_ms(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "time_add_ms", span)?;
    let ms = int_arg(args, 0, "time_add_ms", span)?;
    let delta = int_arg(args, 1, "time_add_ms", span)?;
    Ok(ok_int(ms.saturating_add(delta)))
}

fn time_add_seconds(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "time_add_seconds", span)?;
    let ms = int_arg(args, 0, "time_add_seconds", span)?;
    let delta = int_arg(args, 1, "time_add_seconds", span)?;
    Ok(ok_int(ms.saturating_add(delta.saturating_mul(1000))))
}

fn time_add_minutes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "time_add_minutes", span)?;
    let ms = int_arg(args, 0, "time_add_minutes", span)?;
    let delta = int_arg(args, 1, "time_add_minutes", span)?;
    Ok(ok_int(ms.saturating_add(delta.saturating_mul(60_000))))
}

fn time_add_hours(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "time_add_hours", span)?;
    let ms = int_arg(args, 0, "time_add_hours", span)?;
    let delta = int_arg(args, 1, "time_add_hours", span)?;
    Ok(ok_int(ms.saturating_add(delta.saturating_mul(3_600_000))))
}

fn time_add_days(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "time_add_days", span)?;
    let ms = int_arg(args, 0, "time_add_days", span)?;
    let delta = int_arg(args, 1, "time_add_days", span)?;
    Ok(ok_int(ms.saturating_add(delta.saturating_mul(86_400_000))))
}

fn time_diff_ms(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "time_diff_ms", span)?;
    let a = int_arg(args, 0, "time_diff_ms", span)?;
    let b = int_arg(args, 1, "time_diff_ms", span)?;
    Ok(ok_int(a.saturating_sub(b)))
}

fn time_utc_offset_ms(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "time_utc_offset_ms", span)?;
    let tz = resolve_tz(&string_arg(args, 0, "time_utc_offset_ms", span)?)
        .map_err(|e| type_err(span, e))?;
    let ms = if args.len() == 2 {
        int_arg(args, 1, "time_utc_offset_ms", span)?
    } else {
        system_now_unix_ms()
    };
    match tz.offset_at_ms(ms) {
        Ok(off) => Ok(ok_int(off as i64 * 1000)),
        Err(msg) => Ok(time_error(span, msg)),
    }
}

fn time_is_leap_year(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "time_is_leap_year", span)?;
    let year = int_arg(args, 0, "time_is_leap_year", span)? as i32;
    Ok(ok_bool(is_leap_year(year)))
}

fn time_days_in_month(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "time_days_in_month", span)?;
    let year = int_arg(args, 0, "time_days_in_month", span)? as i32;
    let month = int_arg(args, 1, "time_days_in_month", span)? as u32;
    Ok(ok_int(days_in_month(year, month) as i64))
}

fn time_is_valid_date(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "time_is_valid_date", span)?;
    let year = int_arg(args, 0, "time_is_valid_date", span)? as i32;
    let month = int_arg(args, 1, "time_is_valid_date", span)? as u32;
    let day = int_arg(args, 2, "time_is_valid_date", span)? as u32;
    Ok(ok_bool(is_valid_date(year, month, day)))
}

fn start_of_day_ms(ms: i64, tz: &Timezone) -> Result<i64, String> {
    let fields = view_in_tz(ms, tz)?;
    let civil = CivilDateTime {
        year: fields.year,
        month: fields.month,
        day: fields.day,
        hour: 0,
        minute: 0,
        second: 0,
        millisecond: 0,
    };
    civil_to_ms(&civil, tz)
}

fn time_start_of_day(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "time_start_of_day", span)?;
    let ms = int_arg(args, 0, "time_start_of_day", span)?;
    let tz = if args.len() == 2 {
        resolve_tz(&string_arg(args, 1, "time_start_of_day", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::local()
    };
    match start_of_day_ms(ms, &tz) {
        Ok(v) => Ok(ok_int(v)),
        Err(msg) => Ok(time_error(span, msg)),
    }
}

fn time_end_of_day(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "time_end_of_day", span)?;
    let ms = int_arg(args, 0, "time_end_of_day", span)?;
    let tz = if args.len() == 2 {
        resolve_tz(&string_arg(args, 1, "time_end_of_day", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::local()
    };
    match start_of_day_ms(ms, &tz) {
        Ok(start) => Ok(ok_int(start.saturating_add(86_399_999))),
        Err(msg) => Ok(time_error(span, msg)),
    }
}

fn time_sleep_ms(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "time_sleep_ms", span)?;
    let ms = int_arg(args, 0, "time_sleep_ms", span)?;
    if ms < 0 {
        return Err(type_err(span, "time_sleep_ms() expects a non-negative int"));
    }
    thread::sleep(std::time::Duration::from_millis(ms as u64));
    Ok(ok_nil())
}

fn time_list_timezones(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let zones: Vec<ValueRef> = list_timezones()
        .iter()
        .map(|z| Value::String(z.to_string()).ref_cell())
        .collect();
    Ok(Value::Array(zones).ref_cell())
}

fn time_year(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "time_year", span)?;
    let ms = int_arg(args, 0, "time_year", span)?;
    let tz = if args.len() == 2 {
        resolve_tz(&string_arg(args, 1, "time_year", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::local()
    };
    let fields = view_in_tz(ms, &tz).map_err(|e| type_err(span, e))?;
    Ok(ok_int(fields.year as i64))
}

fn time_month(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "time_month", span)?;
    let ms = int_arg(args, 0, "time_month", span)?;
    let tz = if args.len() == 2 {
        resolve_tz(&string_arg(args, 1, "time_month", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::local()
    };
    let fields = view_in_tz(ms, &tz).map_err(|e| type_err(span, e))?;
    Ok(ok_int(fields.month as i64))
}

fn time_day(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "time_day", span)?;
    let ms = int_arg(args, 0, "time_day", span)?;
    let tz = if args.len() == 2 {
        resolve_tz(&string_arg(args, 1, "time_day", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::local()
    };
    let fields = view_in_tz(ms, &tz).map_err(|e| type_err(span, e))?;
    Ok(ok_int(fields.day as i64))
}

fn time_hour(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "time_hour", span)?;
    let ms = int_arg(args, 0, "time_hour", span)?;
    let tz = if args.len() == 2 {
        resolve_tz(&string_arg(args, 1, "time_hour", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::local()
    };
    let fields = view_in_tz(ms, &tz).map_err(|e| type_err(span, e))?;
    Ok(ok_int(fields.hour as i64))
}

fn time_minute(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "time_minute", span)?;
    let ms = int_arg(args, 0, "time_minute", span)?;
    let tz = if args.len() == 2 {
        resolve_tz(&string_arg(args, 1, "time_minute", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::local()
    };
    let fields = view_in_tz(ms, &tz).map_err(|e| type_err(span, e))?;
    Ok(ok_int(fields.minute as i64))
}

fn time_second(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "time_second", span)?;
    let ms = int_arg(args, 0, "time_second", span)?;
    let tz = if args.len() == 2 {
        resolve_tz(&string_arg(args, 1, "time_second", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::local()
    };
    let fields = view_in_tz(ms, &tz).map_err(|e| type_err(span, e))?;
    Ok(ok_int(fields.second as i64))
}

fn time_weekday(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "time_weekday", span)?;
    let ms = int_arg(args, 0, "time_weekday", span)?;
    let tz = if args.len() == 2 {
        resolve_tz(&string_arg(args, 1, "time_weekday", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::local()
    };
    let fields = view_in_tz(ms, &tz).map_err(|e| type_err(span, e))?;
    Ok(ok_int(fields.weekday as i64))
}

fn time_weekday_name(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "time_weekday_name", span)?;
    let ms = int_arg(args, 0, "time_weekday_name", span)?;
    let tz = if args.len() == 2 {
        resolve_tz(&string_arg(args, 1, "time_weekday_name", span)?).map_err(|e| type_err(span, e))?
    } else {
        Timezone::local()
    };
    let fields = view_in_tz(ms, &tz).map_err(|e| type_err(span, e))?;
    Ok(ok_string(WEEKDAY_NAMES[fields.weekday]))
}

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    vec![
        ("time_now_unix_ms", Rc::new(time_now_unix_ms)),
        ("time_now_perf_us", Rc::new(time_now_perf_us)),
        ("time_elapsed_ms", Rc::new(time_elapsed_ms)),
        ("time_elapsed_us", Rc::new(time_elapsed_us)),
        ("time_elapsed_bench_ms", Rc::new(time_elapsed_bench_ms)),
        ("time_now_unix_s", Rc::new(time_now_unix_s)),
        ("time_now_iso", Rc::new(time_now_iso)),
        ("time_now_iso_local", Rc::new(time_now_iso_local)),
        ("time_now", Rc::new(time_now)),
        ("time_format", Rc::new(time_format)),
        ("time_parse", Rc::new(time_parse)),
        ("time_to_iso", Rc::new(time_to_iso)),
        ("time_decompose", Rc::new(time_decompose)),
        ("time_from_parts", Rc::new(time_from_parts)),
        ("time_add_ms", Rc::new(time_add_ms)),
        ("time_add_seconds", Rc::new(time_add_seconds)),
        ("time_add_minutes", Rc::new(time_add_minutes)),
        ("time_add_hours", Rc::new(time_add_hours)),
        ("time_add_days", Rc::new(time_add_days)),
        ("time_diff_ms", Rc::new(time_diff_ms)),
        ("time_utc_offset_ms", Rc::new(time_utc_offset_ms)),
        ("time_is_leap_year", Rc::new(time_is_leap_year)),
        ("time_days_in_month", Rc::new(time_days_in_month)),
        ("time_is_valid_date", Rc::new(time_is_valid_date)),
        ("time_start_of_day", Rc::new(time_start_of_day)),
        ("time_end_of_day", Rc::new(time_end_of_day)),
        ("time_sleep_ms", Rc::new(time_sleep_ms)),
        ("time_list_timezones", Rc::new(time_list_timezones)),
        ("time_year", Rc::new(time_year)),
        ("time_month", Rc::new(time_month)),
        ("time_day", Rc::new(time_day)),
        ("time_hour", Rc::new(time_hour)),
        ("time_minute", Rc::new(time_minute)),
        ("time_second", Rc::new(time_second)),
        ("time_weekday", Rc::new(time_weekday)),
        ("time_weekday_name", Rc::new(time_weekday_name)),
    ]
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    let bind = |map: &mut HashMap<String, ValueRef>, name: &str, f: NativeFn| {
        map.insert(name.to_string(), Value::NativeFunction(f).ref_cell());
    };
    bind(&mut map, "now_unix_ms", Rc::new(time_now_unix_ms));
    bind(&mut map, "now_perf_us", Rc::new(time_now_perf_us));
    bind(&mut map, "elapsed_ms", Rc::new(time_elapsed_ms));
    bind(&mut map, "elapsed_us", Rc::new(time_elapsed_us));
    bind(&mut map, "elapsed_bench_ms", Rc::new(time_elapsed_bench_ms));
    bind(&mut map, "now_unix_s", Rc::new(time_now_unix_s));
    bind(&mut map, "now_iso", Rc::new(time_now_iso));
    bind(&mut map, "now_iso_local", Rc::new(time_now_iso_local));
    bind(&mut map, "now", Rc::new(time_now));
    bind(&mut map, "format", Rc::new(time_format));
    bind(&mut map, "parse", Rc::new(time_parse));
    bind(&mut map, "to_iso", Rc::new(time_to_iso));
    bind(&mut map, "decompose", Rc::new(time_decompose));
    bind(&mut map, "from_parts", Rc::new(time_from_parts));
    bind(&mut map, "add_ms", Rc::new(time_add_ms));
    bind(&mut map, "add_seconds", Rc::new(time_add_seconds));
    bind(&mut map, "add_minutes", Rc::new(time_add_minutes));
    bind(&mut map, "add_hours", Rc::new(time_add_hours));
    bind(&mut map, "add_days", Rc::new(time_add_days));
    bind(&mut map, "diff_ms", Rc::new(time_diff_ms));
    bind(&mut map, "utc_offset_ms", Rc::new(time_utc_offset_ms));
    bind(&mut map, "is_leap_year", Rc::new(time_is_leap_year));
    bind(&mut map, "days_in_month", Rc::new(time_days_in_month));
    bind(&mut map, "is_valid_date", Rc::new(time_is_valid_date));
    bind(&mut map, "start_of_day", Rc::new(time_start_of_day));
    bind(&mut map, "end_of_day", Rc::new(time_end_of_day));
    bind(&mut map, "sleep_ms", Rc::new(time_sleep_ms));
    bind(&mut map, "list_timezones", Rc::new(time_list_timezones));
    bind(&mut map, "year", Rc::new(time_year));
    bind(&mut map, "month", Rc::new(time_month));
    bind(&mut map, "day", Rc::new(time_day));
    bind(&mut map, "hour", Rc::new(time_hour));
    bind(&mut map, "minute", Rc::new(time_minute));
    bind(&mut map, "second", Rc::new(time_second));
    bind(&mut map, "weekday", Rc::new(time_weekday));
    bind(&mut map, "weekday_name", Rc::new(time_weekday_name));
    Value::Object(map)
}

pub const MODULE_NAME: &str = "time";
pub const MODULE_PATHS: &[&str] = &["time", "std/time"];

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

    #[test]
    fn unix_roundtrip() {
        let ms = system_now_unix_ms();
        let formatted = format_with_tz(ms, "%Y-%m-%d %H:%M:%S", &Timezone::utc()).unwrap();
        let parsed = parse_with_tz(&formatted, "%Y-%m-%d %H:%M:%S", &Timezone::utc()).unwrap();
        assert!((parsed - ms).abs() < 1000);
    }

    #[test]
    fn from_parts_utc() {
        let ms = parse_with_tz("2026-07-03 12:00:00", "%Y-%m-%d %H:%M:%S", &Timezone::utc()).unwrap();
        let args = [
            Value::Int(2026).ref_cell(),
            Value::Int(7).ref_cell(),
            Value::Int(3).ref_cell(),
            Value::Int(12).ref_cell(),
            Value::Int(0).ref_cell(),
            Value::Int(0).ref_cell(),
            Value::Int(0).ref_cell(),
            Value::String("UTC".into()).ref_cell(),
        ];
        let built = time_from_parts(&args, span()).unwrap();
        let got = match &*built.borrow() {
            Value::Int(n) => *n,
            other => panic!("expected int, got {other:?}"),
        };
        assert_eq!(got, ms);
    }

    #[test]
    fn leap_year_and_days() {
        let args = [Value::Int(2024).ref_cell()];
        match &*time_is_leap_year(&args, span()).unwrap().borrow() {
            Value::Bool(b) => assert!(*b),
            other => panic!("expected bool, got {other:?}"),
        }
        let args = [Value::Int(2024).ref_cell(), Value::Int(2).ref_cell()];
        match &*time_days_in_month(&args, span()).unwrap().borrow() {
            Value::Int(n) => assert_eq!(*n, 29),
            other => panic!("expected int, got {other:?}"),
        }
    }
}
