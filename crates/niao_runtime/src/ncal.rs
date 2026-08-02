//! Native ncal standard library — calendar math: business days, holiday tables,
//! week numbers, month grids (~calendar, holidays, workalendar subset).
//!
//! Import with `import "ncal"` (or `import "std/ncal"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_cal::{
    add_business_days, batch_is_weekday, business_days_between_fast, date_range, days_in_month_of,
    diff_days, easter_sunday, format_date, iter_month, leap_year, month_days, month_matrix,
    month_names, month_weeks, next_business_day, nth_weekday_of_month, parse_date,
    prev_business_day, uk_bank_holidays, us_federal_calendar, us_federal_holidays, valid_date,
    week_of_month, weekday_names, Date, WorkCalendar,
};
use niao_errors::codes;
use niao_time::{ms_to_civil, now_unix_ms, Timezone};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E4370: u32 = codes::E4370_NCAL_ARITY;
const E4371: u32 = codes::E4371_NCAL_ERROR;
const E4372: u32 = codes::E4372_NCAL_TYPE;
const E4373: u32 = codes::E4373_NCAL_INVALID_HANDLE;

thread_local! {
    static CALENDARS: RefCell<HashMap<i64, WorkCalendar>> = RefCell::new(HashMap::new());
    static NEXT_CAL_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4372, msg.into())
}

fn ncal_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4371, "ncal_error", msg.into(), span)
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4370,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4370,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn bool_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<bool> {
    match &*args[idx].borrow() {
        Value::Bool(b) => Ok(*b),
        Value::Int(n) => Ok(*n != 0),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects bool as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(m) => Some(m.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn weekend_from_opts(map: Option<&HashMap<String, ValueRef>>) -> Result<Vec<u8>, String> {
    let Some(map) = map else {
        return Ok(vec![5, 6]);
    };
    if let Some(v) = map.get("weekend") {
        match &*v.borrow() {
            Value::Array(items) => {
                let mut days = Vec::new();
                for item in items {
                    match &*item.borrow() {
                        Value::Int(n) => days.push(*n as u8),
                        other => return Err(format!("weekend items must be int, got {}", other.type_name())),
                    }
                }
                return Ok(days);
            }
            other => return Err(format!("weekend must be array, got {}", other.type_name())),
        }
    }
    if let Some(v) = map.get("workweek") {
        match &*v.borrow() {
            Value::Array(items) => {
                let mut work = [false; 7];
                for item in items {
                    match &*item.borrow() {
                        Value::Int(n) if (0..=6).contains(n) => work[*n as usize] = true,
                        other => return Err(format!("workweek items must be 0..=6, got {}", other.type_name())),
                    }
                }
                let weekend: Vec<u8> = (0u8..=6).filter(|d| !work[*d as usize]).collect();
                return Ok(weekend);
            }
            other => return Err(format!("workweek must be array, got {}", other.type_name())),
        }
    }
    Ok(vec![5, 6])
}

fn date_to_object(d: Date) -> Value {
    let (iso_y, iso_w, _) = d.iso_week();
    let mut map = HashMap::new();
    let ins = |m: &mut HashMap<String, ValueRef>, k: &str, v: Value| {
        m.insert(k.to_string(), v.ref_cell());
    };
    ins(&mut map, "year", Value::Int(d.year() as i64));
    ins(&mut map, "month", Value::Int(d.month() as i64));
    ins(&mut map, "day", Value::Int(d.day() as i64));
    ins(&mut map, "weekday", Value::Int(d.weekday() as i64));
    ins(&mut map, "ordinal", Value::Int(d.ordinal() as i64));
    ins(&mut map, "quarter", Value::Int(d.quarter() as i64));
    ins(&mut map, "iso_year", Value::Int(iso_y as i64));
    ins(&mut map, "iso_week", Value::Int(iso_w as i64));
    ins(&mut map, "iso", Value::String(d.format_iso()));
    Value::Object(map)
}

fn date_from_value(v: &ValueRef, span: Span, ctx: &str) -> Result<Date, ValueRef> {
    match &*v.borrow() {
        Value::Object(map) => {
            let year = map.get("year").and_then(|x| match &*x.borrow() {
                Value::Int(n) => Some(*n as i32),
                _ => None,
            });
            let month = map.get("month").and_then(|x| match &*x.borrow() {
                Value::Int(n) => Some(*n as u32),
                _ => None,
            });
            let day = map.get("day").and_then(|x| match &*x.borrow() {
                Value::Int(n) => Some(*n as u32),
                _ => None,
            });
            if let (Some(y), Some(m), Some(d)) = (year, month, day) {
                return Date::new(y, m, d).map_err(|e| ncal_err(span, e.message()));
            }
            if let Some(iso) = map.get("iso").and_then(|x| match &*x.borrow() {
                Value::String(s) => Some(s.clone()),
                _ => None,
            }) {
                return parse_date(&iso).map_err(|e| ncal_err(span, e.message()));
            }
            Err(ncal_err(
                span,
                format!("{ctx}: date object needs year/month/day or iso"),
            ))
        }
        Value::String(s) => parse_date(s).map_err(|e| ncal_err(span, e.message())),
        other => Err(ncal_err(
            span,
            format!("{ctx}: expected date object or string, got {}", other.type_name()),
        )),
    }
}

fn dates_from_array(v: &ValueRef, span: Span, ctx: &str) -> Result<Vec<Date>, ValueRef> {
    match &*v.borrow() {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                date_from_value(item, span, &format!("{ctx}[{i}]"))
            })
            .collect(),
        other => Err(ncal_err(
            span,
            format!("{ctx}: expected array, got {}", other.type_name()),
        )),
    }
}

fn ok_date(d: Date) -> NiaoResult<ValueRef> {
    Ok(date_to_object(d).ref_cell())
}

fn ok_bool(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn ok_int(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

fn ok_string(s: impl Into<String>) -> NiaoResult<ValueRef> {
    Ok(Value::String(s.into()).ref_cell())
}

fn dates_to_array(dates: &[Date]) -> ValueRef {
    Value::Array(dates.iter().map(|d| date_to_object(*d).ref_cell()).collect()).ref_cell()
}

fn matrix_to_array(matrix: Vec<Vec<u32>>) -> ValueRef {
    Value::Array(
        matrix
            .into_iter()
            .map(|row| {
                Value::Array(
                    row.into_iter()
                        .map(|d| Value::Int(d as i64).ref_cell())
                        .collect(),
                )
                .ref_cell()
            })
            .collect(),
    )
    .ref_cell()
}

fn register_cal(cal: WorkCalendar) -> i64 {
    let id = NEXT_CAL_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    CALENDARS.with(|m| m.borrow_mut().insert(id, cal));
    id
}

fn with_cal<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&WorkCalendar) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    CALENDARS.with(|m| {
        match m.borrow().get(&id) {
            Some(c) => Ok(Ok(f(c))),
            None => Ok(Err(error_value(
                E4373,
                "ncal_error",
                format!("invalid or closed ncal calendar handle {id}"),
                span,
            ))),
        }
    })
}

fn with_cal_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut WorkCalendar) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    CALENDARS.with(|m| {
        match m.borrow_mut().get_mut(&id) {
            Some(c) => Ok(Ok(f(c))),
            None => Ok(Err(error_value(
                E4373,
                "ncal_error",
                format!("invalid or closed ncal calendar handle {id}"),
                span,
            ))),
        }
    })
}

fn weekend_from_arg(args: &[ValueRef], idx: usize, span: Span, name: &str) -> Result<Vec<u8>, ValueRef> {
    if args.len() <= idx {
        return Ok(vec![5, 6]);
    }
    match weekend_from_opts(optional_object(args, idx).as_ref()) {
        Ok(w) => Ok(w),
        Err(msg) => Err(ncal_err(span, format!("{name}(): {msg}"))),
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

// >>> ncal.date(2026, 7, 13).iso
// "2026-07-13"
fn ncal_date(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ncal_date", span)?;
    let y = int_arg(args, 0, "ncal_date", span)? as i32;
    let m = int_arg(args, 1, "ncal_date", span)? as u32;
    let d = int_arg(args, 2, "ncal_date", span)? as u32;
    match Date::new(y, m, d) {
        Ok(dt) => ok_date(dt),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

// >>> ncal.parse("2026-07-13").day
// 13
fn ncal_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncal_parse", span)?;
    let text = string_arg(args, 0, "ncal_parse", span)?;
    match parse_date(&text) {
        Ok(d) => ok_date(d),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

// >>> ncal.format({year: 2026, month: 7, day: 13})
// "2026-07-13"
fn ncal_format(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncal_format", span)?;
    let d = match date_from_value(&args[0], span, "ncal_format") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let fmt = if args.len() > 1 {
        string_arg(args, 1, "ncal_format", span)?
    } else {
        "%Y-%m-%d".into()
    };
    ok_string(format_date(&d, &fmt))
}

// >>> ncal.valid(2026, 2, 29)
// false
fn ncal_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ncal_valid", span)?;
    let y = int_arg(args, 0, "ncal_valid", span)? as i32;
    let m = int_arg(args, 1, "ncal_valid", span)? as u32;
    let d = int_arg(args, 2, "ncal_valid", span)? as u32;
    ok_bool(valid_date(y, m, d))
}

// >>> ncal.leap_year(2024)
// true
fn ncal_leap_year(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncal_leap_year", span)?;
    ok_bool(leap_year(int_arg(args, 0, "ncal_leap_year", span)? as i32))
}

// >>> ncal.days_in_month(2026, 2)
// 28
fn ncal_days_in_month(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncal_days_in_month", span)?;
    let y = int_arg(args, 0, "ncal_days_in_month", span)? as i32;
    let m = int_arg(args, 1, "ncal_days_in_month", span)? as u32;
    match days_in_month_of(y, m) {
        Ok(n) => ok_int(n as i64),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

// >>> ncal.weekday({year: 2026, month: 7, day: 13})
// 0
fn ncal_weekday(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncal_weekday", span)?;
    let d = match date_from_value(&args[0], span, "ncal_weekday") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    ok_int(d.weekday() as i64)
}

// >>> ncal.iso_week({year: 2026, month: 1, day: 5}).week
// 2
fn ncal_iso_week(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncal_iso_week", span)?;
    let d = match date_from_value(&args[0], span, "ncal_iso_week") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let (y, w, wd) = d.iso_week();
    let mut map = HashMap::new();
    map.insert("year".into(), Value::Int(y as i64).ref_cell());
    map.insert("week".into(), Value::Int(w as i64).ref_cell());
    map.insert("weekday".into(), Value::Int(wd as i64).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

// >>> ncal.ordinal({year: 2026, month: 7, day: 13})
// 194
fn ncal_ordinal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncal_ordinal", span)?;
    let d = match date_from_value(&args[0], span, "ncal_ordinal") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    ok_int(d.ordinal() as i64)
}

// >>> ncal.quarter({year: 2026, month: 7, day: 13})
// 3
fn ncal_quarter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncal_quarter", span)?;
    let d = match date_from_value(&args[0], span, "ncal_quarter") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    ok_int(d.quarter() as i64)
}

// >>> ncal.add_days({year: 2026, month: 7, day: 13}, 1).day
// 14
fn ncal_add_days(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncal_add_days", span)?;
    let d = match date_from_value(&args[0], span, "ncal_add_days") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let n = int_arg(args, 1, "ncal_add_days", span)? as i32;
    ok_date(d.add_days(n))
}

// >>> ncal.diff_days({year: 2026, month: 7, day: 1}, {year: 2026, month: 7, day: 13})
// 12
fn ncal_diff_days(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncal_diff_days", span)?;
    let a = match date_from_value(&args[0], span, "ncal_diff_days") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let b = match date_from_value(&args[1], span, "ncal_diff_days") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    ok_int(diff_days(a, b) as i64)
}

// >>> len(ncal.range({year: 2026, month: 7, day: 1}, {year: 2026, month: 7, day: 3}))
// 3
fn ncal_range(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncal_range", span)?;
    let a = match date_from_value(&args[0], span, "ncal_range") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let b = match date_from_value(&args[1], span, "ncal_range") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    Ok(dates_to_array(&date_range(a, b)))
}

// >>> ncal.is_weekend({year: 2026, month: 7, day: 11})
// true
fn ncal_is_weekend(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncal_is_weekend", span)?;
    let d = match date_from_value(&args[0], span, "ncal_is_weekend") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let weekend = match niao_cal::weekend_from_days(&weekend_from_arg(args, 1, span, "ncal_is_weekend")?) {
        Ok(w) => w,
        Err(e) => return Ok(ncal_err(span, e.message())),
    };
    ok_bool(niao_cal::is_weekend(d, &weekend))
}

// >>> ncal.is_weekday({year: 2026, month: 7, day: 13})
// true
fn ncal_is_weekday(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncal_is_weekday", span)?;
    let d = match date_from_value(&args[0], span, "ncal_is_weekday") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let weekend = match niao_cal::weekend_from_days(&weekend_from_arg(args, 1, span, "ncal_is_weekday")?) {
        Ok(w) => w,
        Err(e) => return Ok(ncal_err(span, e.message())),
    };
    ok_bool(niao_cal::is_weekday(d, &weekend))
}

// >>> ncal.add_business_days({year: 2026, month: 7, day: 10}, 1).iso
// "2026-07-13"
fn ncal_add_business_days(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncal_add_business_days", span)?;
    let d = match date_from_value(&args[0], span, "ncal_add_business_days") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let n = int_arg(args, 1, "ncal_add_business_days", span)? as i32;
    let weekend = match niao_cal::weekend_from_days(&weekend_from_arg(args, 2, span, "ncal_add_business_days")?) {
        Ok(w) => w,
        Err(e) => return Ok(ncal_err(span, e.message())),
    };
    ok_date(add_business_days(d, n, &weekend))
}

// >>> ncal.business_days_between({year: 2026, month: 7, day: 10}, {year: 2026, month: 7, day: 13})
// 2
fn ncal_business_days_between(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncal_business_days_between", span)?;
    let a = match date_from_value(&args[0], span, "ncal_business_days_between") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let b = match date_from_value(&args[1], span, "ncal_business_days_between") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let weekend = match niao_cal::weekend_from_days(&weekend_from_arg(args, 2, span, "ncal_business_days_between")?) {
        Ok(w) => w,
        Err(e) => return Ok(ncal_err(span, e.message())),
    };
    ok_int(business_days_between_fast(a, b, &weekend) as i64)
}

fn ncal_next_business_day(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "ncal_next_business_day", span)?;
    let d = match date_from_value(&args[0], span, "ncal_next_business_day") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let include = args.len() > 1 && matches!(&*args[1].borrow(), Value::Bool(_) | Value::Int(_))
        && bool_arg(args, 1, "ncal_next_business_day", span).unwrap_or(false);
    let widx = if args.len() > 1 && matches!(&*args[1].borrow(), Value::Object(_) | Value::Nil) {
        1
    } else if args.len() > 2 {
        2
    } else {
        usize::MAX
    };
    let weekend = if widx == usize::MAX {
        niao_cal::default_weekend()
    } else {
        match niao_cal::weekend_from_days(&weekend_from_arg(args, widx, span, "ncal_next_business_day")?) {
            Ok(w) => w,
            Err(e) => return Ok(ncal_err(span, e.message())),
        }
    };
    ok_date(next_business_day(d, &weekend, include))
}

fn ncal_prev_business_day(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "ncal_prev_business_day", span)?;
    let d = match date_from_value(&args[0], span, "ncal_prev_business_day") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let include = args.len() > 1 && matches!(&*args[1].borrow(), Value::Bool(_) | Value::Int(_))
        && bool_arg(args, 1, "ncal_prev_business_day", span).unwrap_or(false);
    let widx = if args.len() > 1 && matches!(&*args[1].borrow(), Value::Object(_) | Value::Nil) {
        1
    } else if args.len() > 2 {
        2
    } else {
        usize::MAX
    };
    let weekend = if widx == usize::MAX {
        niao_cal::default_weekend()
    } else {
        match niao_cal::weekend_from_days(&weekend_from_arg(args, widx, span, "ncal_prev_business_day")?) {
            Ok(w) => w,
            Err(e) => return Ok(ncal_err(span, e.message())),
        }
    };
    ok_date(prev_business_day(d, &weekend, include))
}

// >>> len(ncal.month_days(2026, 7))
// 31
fn ncal_month_days(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncal_month_days", span)?;
    let y = int_arg(args, 0, "ncal_month_days", span)? as i32;
    let m = int_arg(args, 1, "ncal_month_days", span)? as u32;
    match month_days(y, m) {
        Ok(days) => Ok(Value::Array(days.into_iter().map(|d| Value::Int(d as i64).ref_cell()).collect()).ref_cell()),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

// >>> len(ncal.month_matrix(2026, 7))
// 5
fn ncal_month_matrix(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncal_month_matrix", span)?;
    let y = int_arg(args, 0, "ncal_month_matrix", span)? as i32;
    let m = int_arg(args, 1, "ncal_month_matrix", span)? as u32;
    let first = if args.len() > 2 {
        int_arg(args, 2, "ncal_month_matrix", span)? as u8
    } else {
        0
    };
    match month_matrix(y, m, first) {
        Ok(mat) => Ok(matrix_to_array(mat)),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

fn ncal_month_weeks(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncal_month_weeks", span)?;
    let y = int_arg(args, 0, "ncal_month_weeks", span)? as i32;
    let m = int_arg(args, 1, "ncal_month_weeks", span)? as u32;
    let first = if args.len() > 2 {
        int_arg(args, 2, "ncal_month_weeks", span)? as u8
    } else {
        0
    };
    match month_weeks(y, m, first) {
        Ok(n) => ok_int(n as i64),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

fn ncal_iter_month(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncal_iter_month", span)?;
    let y = int_arg(args, 0, "ncal_iter_month", span)? as i32;
    let m = int_arg(args, 1, "ncal_iter_month", span)? as u32;
    match iter_month(y, m) {
        Ok(days) => Ok(dates_to_array(&days)),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

fn ncal_nth_weekday(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "ncal_nth_weekday", span)?;
    let y = int_arg(args, 0, "ncal_nth_weekday", span)? as i32;
    let m = int_arg(args, 1, "ncal_nth_weekday", span)? as u32;
    let wd = int_arg(args, 2, "ncal_nth_weekday", span)? as u8;
    let nth = int_arg(args, 3, "ncal_nth_weekday", span)? as i32;
    match nth_weekday_of_month(y, m, wd, nth) {
        Ok(d) => ok_date(d),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

fn ncal_week_of_month(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncal_week_of_month", span)?;
    let d = match date_from_value(&args[0], span, "ncal_week_of_month") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let first = if args.len() > 1 {
        int_arg(args, 1, "ncal_week_of_month", span)? as u8
    } else {
        0
    };
    match week_of_month(d, first) {
        Ok(n) => ok_int(n as i64),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

// >>> len(ncal.weekdays())
// 7
fn ncal_weekdays(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncal_weekdays", span)?;
    Ok(Value::Array(
        weekday_names()
            .iter()
            .map(|s| Value::String((*s).into()).ref_cell())
            .collect(),
    )
    .ref_cell())
}

// >>> len(ncal.months())
// 12
fn ncal_months(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncal_months", span)?;
    Ok(Value::Array(
        month_names()
            .iter()
            .map(|s| Value::String((*s).into()).ref_cell())
            .collect(),
    )
    .ref_cell())
}

// >>> ncal.today("UTC").year > 2000
// true
fn ncal_today(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "ncal_today", span)?;
    let tz_name = if args.is_empty() {
        "UTC".into()
    } else {
        string_arg(args, 0, "ncal_today", span)?
    };
    let tz = match Timezone::named(&tz_name) {
        Ok(t) => t,
        Err(e) => return Ok(ncal_err(span, e)),
    };
    let ms = now_unix_ms();
    let civil = match ms_to_civil(ms, &tz) {
        Ok(c) => c,
        Err(e) => return Ok(ncal_err(span, e)),
    };
    match Date::new(civil.year, civil.month, civil.day) {
        Ok(d) => ok_date(d),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

// >>> ncal.easter(2026).iso
// "2026-04-05"
fn ncal_easter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncal_easter", span)?;
    let y = int_arg(args, 0, "ncal_easter", span)? as i32;
    match easter_sunday(y) {
        Ok(d) => ok_date(d),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

// >>> len(ncal.us_federal(2026)) >= 10
// true
fn ncal_us_federal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncal_us_federal", span)?;
    let y = int_arg(args, 0, "ncal_us_federal", span)? as i32;
    match us_federal_holidays(y) {
        Ok(days) => Ok(dates_to_array(&days)),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

fn ncal_uk_bank(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncal_uk_bank", span)?;
    let y = int_arg(args, 0, "ncal_uk_bank", span)? as i32;
    match uk_bank_holidays(y) {
        Ok(days) => Ok(dates_to_array(&days)),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

fn ncal_batch_is_weekday(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncal_batch_is_weekday", span)?;
    let dates = match dates_from_array(&args[0], span, "ncal_batch_is_weekday") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let weekend = match niao_cal::weekend_from_days(&weekend_from_arg(args, 1, span, "ncal_batch_is_weekday")?) {
        Ok(w) => w,
        Err(e) => return Ok(ncal_err(span, e.message())),
    };
    Ok(Value::Array(
        batch_is_weekday(&dates, &weekend)
            .into_iter()
            .map(|b| Value::Bool(b).ref_cell())
            .collect(),
    )
    .ref_cell())
}

// ---------------------------------------------------------------------------
// Work calendar handles
// ---------------------------------------------------------------------------

// >>> ncal.calendar() > 0
// true
fn ncal_calendar(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "ncal_calendar", span)?;
    let opts = optional_object(args, 0);
    let weekend = match weekend_from_opts(opts.as_ref()) {
        Ok(w) => w,
        Err(msg) => return Ok(ncal_err(span, msg)),
    };
    match WorkCalendar::new(&weekend) {
        Ok(mut cal) => {
            if let Some(map) = opts.as_ref() {
                if let Some(v) = map.get("holidays") {
                    let days = match dates_from_array(v, span, "ncal_calendar holidays") {
                        Ok(d) => d,
                        Err(e) => return Ok(e),
                    };
                    for d in days {
                        cal.add_holiday(d);
                    }
                }
                if let Some(v) = map.get("year") {
                    if let Value::Int(y) = &*v.borrow() {
                        if let Ok(us) = us_federal_holidays(*y as i32) {
                            for d in us {
                                cal.add_holiday(d);
                            }
                        }
                    }
                }
                if let Some(v) = map.get("preset") {
                    if let Value::String(name) = &*v.borrow() {
                        match name.as_str() {
                            "us_federal" => {
                                let y = map
                                    .get("year")
                                    .and_then(|x| match &*x.borrow() {
                                        Value::Int(n) => Some(*n as i32),
                                        _ => None,
                                    })
                                    .unwrap_or(2026);
                                if let Ok(us) = us_federal_holidays(y) {
                                    for d in us {
                                        cal.add_holiday(d);
                                    }
                                }
                            }
                            "uk_bank" => {
                                let y = map
                                    .get("year")
                                    .and_then(|x| match &*x.borrow() {
                                        Value::Int(n) => Some(*n as i32),
                                        _ => None,
                                    })
                                    .unwrap_or(2026);
                                if let Ok(uk) = uk_bank_holidays(y) {
                                    for d in uk {
                                        cal.add_holiday(d);
                                    }
                                }
                            }
                            other => return Ok(ncal_err(span, format!("unknown preset '{other}'"))),
                        }
                    }
                }
            }
            ok_int(register_cal(cal))
        }
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

fn ncal_us_federal_calendar(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncal_us_federal_calendar", span)?;
    let y = int_arg(args, 0, "ncal_us_federal_calendar", span)? as i32;
    match us_federal_calendar(y) {
        Ok(cal) => ok_int(register_cal(cal)),
        Err(e) => Ok(ncal_err(span, e.message())),
    }
}

fn ncal_cal_add_holiday(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncal_cal_add_holiday", span)?;
    let id = int_arg(args, 0, "ncal_cal_add_holiday", span)?;
    let d = match date_from_value(&args[1], span, "ncal_cal_add_holiday") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    match with_cal_mut(id, span, |c| c.add_holiday(d))? {
        Ok(()) => ok_bool(true),
        Err(v) => Ok(v),
    }
}

fn ncal_cal_add_holidays(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncal_cal_add_holidays", span)?;
    let id = int_arg(args, 0, "ncal_cal_add_holidays", span)?;
    let dates = match dates_from_array(&args[1], span, "ncal_cal_add_holidays") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    match with_cal_mut(id, span, |c| {
        let n = dates.len();
        for d in dates {
            c.add_holiday(d);
        }
        n
    })? {
        Ok(n) => ok_int(n as i64),
        Err(v) => Ok(v),
    }
}

fn ncal_cal_remove_holiday(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncal_cal_remove_holiday", span)?;
    let id = int_arg(args, 0, "ncal_cal_remove_holiday", span)?;
    let d = match date_from_value(&args[1], span, "ncal_cal_remove_holiday") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    match with_cal_mut(id, span, |c| c.remove_holiday(d))? {
        Ok(removed) => ok_bool(removed),
        Err(v) => Ok(v),
    }
}

fn ncal_cal_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncal_cal_clear", span)?;
    let id = int_arg(args, 0, "ncal_cal_clear", span)?;
    match with_cal_mut(id, span, |c| c.clear_holidays())? {
        Ok(()) => ok_bool(true),
        Err(v) => Ok(v),
    }
}

fn ncal_cal_is_holiday(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncal_cal_is_holiday", span)?;
    let id = int_arg(args, 0, "ncal_cal_is_holiday", span)?;
    let d = match date_from_value(&args[1], span, "ncal_cal_is_holiday") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    match with_cal(id, span, |c| c.is_holiday(d))? {
        Ok(b) => ok_bool(b),
        Err(v) => Ok(v),
    }
}

fn ncal_cal_is_working(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncal_cal_is_working", span)?;
    let id = int_arg(args, 0, "ncal_cal_is_working", span)?;
    let d = match date_from_value(&args[1], span, "ncal_cal_is_working") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    match with_cal(id, span, |c| c.is_working_day(d))? {
        Ok(b) => ok_bool(b),
        Err(v) => Ok(v),
    }
}

fn ncal_cal_holidays(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncal_cal_holidays", span)?;
    let id = int_arg(args, 0, "ncal_cal_holidays", span)?;
    let y = int_arg(args, 1, "ncal_cal_holidays", span)? as i32;
    match with_cal(id, span, |c| c.holidays_in_year(y))? {
        Ok(days) => Ok(dates_to_array(&days)),
        Err(v) => Ok(v),
    }
}

fn ncal_cal_add_working(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ncal_cal_add_working", span)?;
    let id = int_arg(args, 0, "ncal_cal_add_working", span)?;
    let d = match date_from_value(&args[1], span, "ncal_cal_add_working") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let n = int_arg(args, 2, "ncal_cal_add_working", span)? as i32;
    match with_cal(id, span, |c| c.add_working_days(d, n))? {
        Ok(out) => ok_date(out),
        Err(v) => Ok(v),
    }
}

fn ncal_cal_working_between(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ncal_cal_working_between", span)?;
    let id = int_arg(args, 0, "ncal_cal_working_between", span)?;
    let a = match date_from_value(&args[1], span, "ncal_cal_working_between") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let b = match date_from_value(&args[2], span, "ncal_cal_working_between") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    match with_cal(id, span, |c| c.working_days_between(a, b))? {
        Ok(n) => ok_int(n as i64),
        Err(v) => Ok(v),
    }
}

fn ncal_cal_next_working(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncal_cal_next_working", span)?;
    let id = int_arg(args, 0, "ncal_cal_next_working", span)?;
    let d = match date_from_value(&args[1], span, "ncal_cal_next_working") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let include = args.len() > 2 && bool_arg(args, 2, "ncal_cal_next_working", span)?;
    match with_cal(id, span, |c| c.next_working_day(d, include))? {
        Ok(out) => ok_date(out),
        Err(v) => Ok(v),
    }
}

fn ncal_cal_prev_working(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncal_cal_prev_working", span)?;
    let id = int_arg(args, 0, "ncal_cal_prev_working", span)?;
    let d = match date_from_value(&args[1], span, "ncal_cal_prev_working") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    let include = args.len() > 2 && bool_arg(args, 2, "ncal_cal_prev_working", span)?;
    match with_cal(id, span, |c| c.prev_working_day(d, include))? {
        Ok(out) => ok_date(out),
        Err(v) => Ok(v),
    }
}

fn ncal_cal_batch_working(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncal_cal_batch_working", span)?;
    let id = int_arg(args, 0, "ncal_cal_batch_working", span)?;
    let dates = match dates_from_array(&args[1], span, "ncal_cal_batch_working") {
        Ok(d) => d,
        Err(v) => return Ok(v),
    };
    match with_cal(id, span, |c| c.batch_is_working(&dates))? {
        Ok(flags) => Ok(Value::Array(flags.into_iter().map(|b| Value::Bool(b).ref_cell()).collect()).ref_cell()),
        Err(v) => Ok(v),
    }
}

fn ncal_cal_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncal_cal_count", span)?;
    let id = int_arg(args, 0, "ncal_cal_count", span)?;
    match with_cal(id, span, |c| c.holiday_count())? {
        Ok(n) => ok_int(n as i64),
        Err(v) => Ok(v),
    }
}

fn ncal_cal_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncal_cal_close", span)?;
    let id = int_arg(args, 0, "ncal_cal_close", span)?;
    let removed = CALENDARS.with(|m| m.borrow_mut().remove(&id).is_some());
    ok_bool(removed)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncal_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncal_fns![
    ("ncal_date", "date", ncal_date),
    ("ncal_parse", "parse", ncal_parse),
    ("ncal_format", "format", ncal_format),
    ("ncal_valid", "valid", ncal_valid),
    ("ncal_leap_year", "leap_year", ncal_leap_year),
    ("ncal_days_in_month", "days_in_month", ncal_days_in_month),
    ("ncal_weekday", "weekday", ncal_weekday),
    ("ncal_iso_week", "iso_week", ncal_iso_week),
    ("ncal_ordinal", "ordinal", ncal_ordinal),
    ("ncal_quarter", "quarter", ncal_quarter),
    ("ncal_add_days", "add_days", ncal_add_days),
    ("ncal_diff_days", "diff_days", ncal_diff_days),
    ("ncal_range", "range", ncal_range),
    ("ncal_is_weekend", "is_weekend", ncal_is_weekend),
    ("ncal_is_weekday", "is_weekday", ncal_is_weekday),
    ("ncal_add_business_days", "add_business_days", ncal_add_business_days),
    ("ncal_business_days_between", "business_days_between", ncal_business_days_between),
    ("ncal_next_business_day", "next_business_day", ncal_next_business_day),
    ("ncal_prev_business_day", "prev_business_day", ncal_prev_business_day),
    ("ncal_month_days", "month_days", ncal_month_days),
    ("ncal_month_matrix", "month_matrix", ncal_month_matrix),
    ("ncal_month_weeks", "month_weeks", ncal_month_weeks),
    ("ncal_iter_month", "iter_month", ncal_iter_month),
    ("ncal_nth_weekday", "nth_weekday", ncal_nth_weekday),
    ("ncal_week_of_month", "week_of_month", ncal_week_of_month),
    ("ncal_weekdays", "weekdays", ncal_weekdays),
    ("ncal_months", "months", ncal_months),
    ("ncal_today", "today", ncal_today),
    ("ncal_easter", "easter", ncal_easter),
    ("ncal_us_federal", "us_federal", ncal_us_federal),
    ("ncal_uk_bank", "uk_bank", ncal_uk_bank),
    ("ncal_batch_is_weekday", "batch_is_weekday", ncal_batch_is_weekday),
    ("ncal_calendar", "calendar", ncal_calendar),
    ("ncal_us_federal_calendar", "us_federal_calendar", ncal_us_federal_calendar),
    ("ncal_cal_add_holiday", "cal_add_holiday", ncal_cal_add_holiday),
    ("ncal_cal_add_holidays", "cal_add_holidays", ncal_cal_add_holidays),
    ("ncal_cal_remove_holiday", "cal_remove_holiday", ncal_cal_remove_holiday),
    ("ncal_cal_clear", "cal_clear", ncal_cal_clear),
    ("ncal_cal_is_holiday", "cal_is_holiday", ncal_cal_is_holiday),
    ("ncal_cal_is_working", "cal_is_working", ncal_cal_is_working),
    ("ncal_cal_holidays", "cal_holidays", ncal_cal_holidays),
    ("ncal_cal_add_working", "cal_add_working", ncal_cal_add_working),
    ("ncal_cal_working_between", "cal_working_between", ncal_cal_working_between),
    ("ncal_cal_next_working", "cal_next_working", ncal_cal_next_working),
    ("ncal_cal_prev_working", "cal_prev_working", ncal_cal_prev_working),
    ("ncal_cal_batch_working", "cal_batch_working", ncal_cal_batch_working),
    ("ncal_cal_count", "cal_count", ncal_cal_count),
    ("ncal_cal_close", "cal_close", ncal_cal_close),
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

pub const MODULE_NAME: &str = "ncal";
pub const MODULE_PATHS: &[&str] = &["ncal", "std/ncal"];

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
    fn parse_iso() {
        let args = [Value::String("2026-07-13".into()).ref_cell()];
        let v = ncal_parse(&args, span()).unwrap();
        match &*v.borrow() {
            Value::Object(m) => {
                assert_eq!(
                    match &*m["day"].borrow() {
                        Value::Int(n) => *n,
                        _ => 0,
                    },
                    13
                );
            }
            other => panic!("{other:?}"),
        }
    }
}
