//! Native nical standard library — iCalendar / vCard parse + generate, RRULE
//! recurrence (~icalendar, vobject subset).
//!
//! Import with `import "nical"` (or `import "std/nical"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_ical::{
    calendar, contact, emit, emit_all, emit_rrule, is_valid, parse, parse_all, parse_calendar,
    parse_contacts, parse_ical_datetime, parse_rrule, rrule_from_map, rrule_occurrences,
    rrule_to_map, unix_ms_to_ical, Component, EmitOptions, EventBuilder, IcalError, ParseOptions,
    Property, RRule, Weekday, MAX_BYTES,
};
use std::collections::HashMap;
use std::fs;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4332_NICAL_TYPE, msg.into())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4330_NICAL_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nical_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4331_NICAL_ERROR, "nical_error", msg.into(), span)
}

fn map_ical_err(span: Span, err: IcalError) -> ValueRef {
    let code = match &err {
        IcalError::InvalidProperty { .. }
        | IcalError::UnbalancedComponent { .. }
        | IcalError::UnexpectedEnd { .. }
        | IcalError::InvalidDateTime(_)
        | IcalError::InvalidRrule(_) => codes::E4333_NICAL_PARSE,
        _ => codes::E4331_NICAL_ERROR,
    };
    error_value(code, "nical_error", err.message(), span)
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

fn string_field(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        Some(Value::Int(n)) => Some(n.to_string()),
        Some(Value::Float(f)) => Some(f.to_string()),
        _ => None,
    }
}

fn int_field_opt(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<i64> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => Some(n),
        _ => None,
    }
}

fn parse_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> ParseOptions {
    ParseOptions {
        relaxed: bool_field(map, "relaxed", false),
    }
}

fn emit_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> EmitOptions {
    EmitOptions {
        fold_lines: bool_field(map, "fold_lines", true),
        crlf: bool_field(map, "crlf", true),
    }
}

// ---------------------------------------------------------------------------
// Component ↔ Niao Value bridge
// ---------------------------------------------------------------------------

fn property_to_niao(p: &Property) -> Value {
    let mut params = HashMap::new();
    for (k, vals) in &p.params {
        let arr: Vec<ValueRef> = vals
            .iter()
            .map(|v| Value::String(v.clone()).ref_cell())
            .collect();
        params.insert(k.to_ascii_lowercase(), Value::Array(arr).ref_cell());
    }
    let mut m = HashMap::new();
    m.insert("name".into(), Value::String(p.name.clone()).ref_cell());
    m.insert("value".into(), Value::String(p.value.clone()).ref_cell());
    m.insert("params".into(), Value::Object(params).ref_cell());
    Value::Object(m)
}

fn component_to_niao(c: &Component) -> Value {
    let mut props = Vec::new();
    let mut prop_map = HashMap::new();
    for p in &c.properties {
        props.push(property_to_niao(p).ref_cell());
        prop_map
            .entry(p.name.to_ascii_lowercase())
            .or_insert_with(|| Value::String(p.value.clone()).ref_cell());
    }
    let mut children = Vec::new();
    let mut events = Vec::new();
    let mut todos = Vec::new();
    let mut alarms = Vec::new();
    for ch in &c.children {
        children.push(component_to_niao(ch).ref_cell());
        match ch.name.as_str() {
            "VEVENT" => events.push(component_to_niao(ch).ref_cell()),
            "VTODO" => todos.push(component_to_niao(ch).ref_cell()),
            "VALARM" => alarms.push(component_to_niao(ch).ref_cell()),
            _ => {}
        }
    }
    let kind = match c.name.as_str() {
        "VCALENDAR" => "calendar",
        "VCARD" => "contact",
        "VEVENT" => "event",
        "VTODO" => "todo",
        "VALARM" => "alarm",
        other => other,
    };
    let mut m = HashMap::new();
    m.insert("kind".into(), Value::String(kind.into()).ref_cell());
    m.insert("name".into(), Value::String(c.name.clone()).ref_cell());
    m.insert("properties".into(), Value::Array(props).ref_cell());
    m.insert("props".into(), Value::Object(prop_map).ref_cell());
    m.insert("children".into(), Value::Array(children).ref_cell());
    if !events.is_empty() {
        m.insert("events".into(), Value::Array(events).ref_cell());
    }
    if !todos.is_empty() {
        m.insert("todos".into(), Value::Array(todos).ref_cell());
    }
    if !alarms.is_empty() {
        m.insert("alarms".into(), Value::Array(alarms).ref_cell());
    }
    Value::Object(m)
}

fn components_to_niao_all(items: Vec<Component>) -> Value {
    let arr: Vec<ValueRef> = items.iter().map(|c| component_to_niao(c).ref_cell()).collect();
    Value::Array(arr)
}

fn niao_to_property(v: &ValueRef) -> Result<Property, String> {
    let borrowed = v.borrow();
    let obj = match &*borrowed {
        Value::Object(m) => m,
        _ => return Err("property must be an object".into()),
    };
    let name = obj
        .get("name")
        .and_then(|x| match &*x.borrow() {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .ok_or_else(|| "property.name required".to_string())?;
    let value = obj
        .get("value")
        .and_then(|x| match &*x.borrow() {
            Value::String(s) => Some(s.clone()),
            Value::Int(n) => Some(n.to_string()),
            _ => None,
        })
        .unwrap_or_default();
    let mut prop = Property::new(name, value);
    if let Some(params) = obj.get("params") {
        if let Value::Object(pm) = &*params.borrow() {
            for (k, pv) in pm {
                match &*pv.borrow() {
                    Value::String(s) => {
                        prop = prop.with_param(k.clone(), s.clone());
                    }
                    Value::Array(vals) => {
                        for item in vals {
                            if let Value::String(s) = &*item.borrow() {
                                prop = prop.with_param(k.clone(), s.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(prop)
}

fn niao_to_component(v: &ValueRef) -> Result<Component, String> {
    let borrowed = v.borrow();
    let obj = match &*borrowed {
        Value::Object(m) => m,
        _ => return Err("component must be an object".into()),
    };
    let name = obj
        .get("name")
        .and_then(|x| match &*x.borrow() {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .ok_or_else(|| "component.name required".to_string())?;
    let mut comp = Component::new(name);
    if let Some(props) = obj.get("properties") {
        if let Value::Array(arr) = &*props.borrow() {
            for p in arr {
                comp = comp.with_property(niao_to_property(p)?);
            }
        }
    } else if let Some(props) = obj.get("props") {
        if let Value::Object(pm) = &*props.borrow() {
            for (k, v) in pm {
                let val = match &*v.borrow() {
                    Value::String(s) => s.clone(),
                    Value::Int(n) => n.to_string(),
                    _ => continue,
                };
                comp = comp.with_property(Property::new(k.to_ascii_uppercase(), val));
            }
        }
    }
    if let Some(children) = obj.get("children") {
        if let Value::Array(arr) = &*children.borrow() {
            for ch in arr {
                comp = comp.with_child(niao_to_component(ch)?);
            }
        }
    }
    if let Some(events) = obj.get("events") {
        if let Value::Array(arr) = &*events.borrow() {
            for ev in arr {
                comp = comp.with_child(niao_to_component(ev)?);
            }
        }
    }
    Ok(comp)
}

fn rrule_to_niao(r: &RRule) -> Value {
    let m = rrule_to_map(r);
    let mut out = HashMap::new();
    for (k, v) in m {
        out.insert(k, Value::String(v).ref_cell());
    }
    out.insert(
        "wkst".into(),
        Value::String(weekday_to_str(r.wkst).to_ascii_lowercase()).ref_cell(),
    );
    Value::Object(out)
}

fn weekday_to_str(w: Weekday) -> &'static str {
    match w {
        Weekday::Mo => "MO",
        Weekday::Tu => "TU",
        Weekday::We => "WE",
        Weekday::Th => "TH",
        Weekday::Fr => "FR",
        Weekday::Sa => "SA",
        Weekday::Su => "SU",
    }
}

fn niao_to_rrule(v: &ValueRef) -> Result<RRule, String> {
    match &*v.borrow() {
        Value::String(s) => parse_rrule(s).map_err(|e| e.message()),
        Value::Object(m) => {
            let mut map = HashMap::new();
            for (k, v) in m {
                if let Value::String(s) = &*v.borrow() {
                    map.insert(k.clone(), s.clone());
                } else if let Value::Int(n) = &*v.borrow() {
                    map.insert(k.clone(), n.to_string());
                }
            }
            rrule_from_map(&map).map_err(|e| e.message())
        }
        other => Err(format!("expected string or object for rrule, got {}", other.type_name())),
    }
}

fn event_from_map(m: &HashMap<String, ValueRef>) -> Result<Component, String> {
    let mut b = niao_ical::EventBuilder::default();
    if let Some(s) = string_field(m, "summary") {
        b = b.summary(s);
    }
    if let Some(s) = string_field(m, "uid") {
        b = b.uid(s);
    }
    if let Some(s) = string_field(m, "dtstart") {
        b = b.dtstart(s);
    }
    if let Some(s) = string_field(m, "dtend") {
        b = b.dtend(s);
    }
    if let Some(s) = string_field(m, "location") {
        b = b.location(s);
    }
    if let Some(s) = string_field(m, "description") {
        b = b.description(s);
    }
    if let Some(s) = string_field(m, "rrule") {
        b = b.rrule(s);
    }
    Ok(b.build())
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> nical.parse_calendar("BEGIN:VCALENDAR\nVERSION:2.0\nEND:VCALENDAR\n")
// => {kind: "calendar", name: "VCALENDAR", ...}
fn nical_parse_calendar(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nical_parse_calendar", span)?;
    let text = string_arg(args, 0, "nical_parse_calendar", span)?;
    let opts = parse_opts_from_map(optional_object_arg(args, 1).as_ref());
    match parse_calendar(&text, &opts) {
        Ok(c) => Ok(component_to_niao(&c).ref_cell()),
        Err(e) => Ok(map_ical_err(span, e)),
    }
}

// >>> len(nical.parse_contacts("BEGIN:VCARD\nVERSION:4.0\nFN:A\nEND:VCARD\n"))
// => 1
fn nical_parse_contacts(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nical_parse_contacts", span)?;
    let text = string_arg(args, 0, "nical_parse_contacts", span)?;
    let opts = parse_opts_from_map(optional_object_arg(args, 1).as_ref());
    match parse_contacts(&text, &opts) {
        Ok(v) => Ok(components_to_niao_all(v).ref_cell()),
        Err(e) => Ok(map_ical_err(span, e)),
    }
}

// >>> nical.parse("BEGIN:VCARD\nVERSION:4.0\nFN:X\nEND:VCARD\n").kind
// => "contact"
fn nical_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nical_parse", span)?;
    let text = string_arg(args, 0, "nical_parse", span)?;
    let opts = parse_opts_from_map(optional_object_arg(args, 1).as_ref());
    match parse(&text, &opts) {
        Ok(c) => Ok(component_to_niao(&c).ref_cell()),
        Err(e) => Ok(map_ical_err(span, e)),
    }
}

fn nical_parse_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nical_parse_all", span)?;
    let text = string_arg(args, 0, "nical_parse_all", span)?;
    let opts = parse_opts_from_map(optional_object_arg(args, 1).as_ref());
    match parse_all(&text, &opts) {
        Ok(v) => Ok(components_to_niao_all(v).ref_cell()),
        Err(e) => Ok(map_ical_err(span, e)),
    }
}

// >>> nical.valid("BEGIN:VCALENDAR\nVERSION:2.0\nEND:VCALENDAR\n")
// => true
fn nical_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nical_valid", span)?;
    let text = string_arg(args, 0, "nical_valid", span)?;
    Ok(Value::Bool(is_valid(&text)).ref_cell())
}

fn nical_parse_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nical_parse_file", span)?;
    let path = string_arg(args, 0, "nical_parse_file", span)?;
    let text = fs::read_to_string(&path).map_err(|e| {
        RuntimeError::at(
            span,
            codes::E4331_NICAL_ERROR,
            format!("nical_parse_file: cannot read '{path}': {e}"),
        )
    })?;
    if text.len() > MAX_BYTES {
        return Ok(map_ical_err(span, IcalError::TooLarge(text.len())));
    }
    let mut file_args = vec![Value::String(text).ref_cell()];
    if args.len() > 1 {
        file_args.push(args[1].clone());
    }
    nical_parse(&file_args, span)
}

// >>> nical.emit({name: "VCARD", props: {FN: "Ada"}}).find("FN:Ada") >= 0
// => true
fn nical_emit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nical_emit", span)?;
    let comp = match niao_to_component(&args[0]) {
        Ok(c) => c,
        Err(msg) => return Ok(nical_err(span, msg)),
    };
    let opts = emit_opts_from_map(optional_object_arg(args, 1).as_ref());
    match emit(&comp, &opts) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_ical_err(span, e)),
    }
}

fn nical_emit_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nical_emit_all", span)?;
    let arr = match &*args[0].borrow() {
        Value::Array(items) => items.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "nical_emit_all() expects an array as argument 1, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    let mut comps = Vec::with_capacity(arr.len());
    for item in &arr {
        comps.push(niao_to_component(item).map_err(|msg| {
            RuntimeError::at(span, codes::E4332_NICAL_TYPE, msg)
        })?);
    }
    let opts = emit_opts_from_map(optional_object_arg(args, 1).as_ref());
    match emit_all(&comps, &opts) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_ical_err(span, e)),
    }
}

fn nical_emit_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nical_emit_file", span)?;
    let path = string_arg(args, 0, "nical_emit_file", span)?;
    let emit_args = if args.len() > 2 {
        vec![args[1].clone(), args[2].clone()]
    } else {
        vec![args[1].clone()]
    };
    let out = nical_emit(&emit_args, span)?;
    let is_err = matches!(&*out.borrow(), Value::Error { .. });
    if is_err {
        return Ok(out);
    }
    let text = match &*out.borrow() {
        Value::String(s) => s.clone(),
        other => {
            return Err(type_err(
                span,
                format!("nical_emit_file: expected string from emit, got {}", other.type_name()),
            ))
        }
    };
    fs::write(&path, text).map_err(|e| {
        RuntimeError::at(
            span,
            codes::E4331_NICAL_ERROR,
            format!("nical_emit_file: cannot write '{path}': {e}"),
        )
    })?;
    Ok(Value::Bool(true).ref_cell())
}

// >>> nical.parse_rrule("FREQ=DAILY;COUNT=3").freq
// => "daily"
fn nical_parse_rrule(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nical_parse_rrule", span)?;
    let text = string_arg(args, 0, "nical_parse_rrule", span)?;
    match parse_rrule(&text) {
        Ok(r) => Ok(rrule_to_niao(&r).ref_cell()),
        Err(e) => Ok(map_ical_err(span, e)),
    }
}

// >>> nical.emit_rrule({freq: "daily", interval: 1, count: 2})
// => "FREQ=DAILY;INTERVAL=1;COUNT=2"
fn nical_emit_rrule(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nical_emit_rrule", span)?;
    match niao_to_rrule(&args[0]) {
        Ok(r) => Ok(Value::String(emit_rrule(&r)).ref_cell()),
        Err(msg) => Ok(nical_err(span, msg)),
    }
}

// >>> len(nical.rrule_between("FREQ=WEEKLY;BYDAY=MO;COUNT=3", "20260105T090000Z", nil, nil, 3))
// => 3
fn nical_rrule_between(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 5, "nical_rrule_between", span)?;
    let rule = match niao_to_rrule(&args[0]) {
        Ok(r) => r,
        Err(msg) => return Ok(nical_err(span, msg)),
    };
    let dtstart = string_arg(args, 1, "nical_rrule_between", span)?;
    let after = if args.len() > 2 {
        match &*args[2].borrow() {
            Value::Int(n) => Some(*n),
            Value::Nil => None,
            _ => None,
        }
    } else {
        None
    };
    let before = if args.len() > 3 {
        match &*args[3].borrow() {
            Value::Int(n) => Some(*n),
            Value::Nil => None,
            _ => None,
        }
    } else {
        None
    };
    let max_count = if args.len() > 4 {
        match &*args[4].borrow() {
            Value::Int(n) if *n > 0 => Some(*n as usize),
            Value::Nil => None,
            _ => None,
        }
    } else {
        None
    };
    match rrule_occurrences(&rule, &dtstart, after, before, max_count) {
        Ok(ms) => {
            let arr: Vec<ValueRef> = ms.into_iter().map(|n| Value::Int(n).ref_cell()).collect();
            Ok(Value::Array(arr).ref_cell())
        }
        Err(e) => Ok(map_ical_err(span, e)),
    }
}

// >>> nical.parse_datetime("20260105T090000Z").utc
// => true
fn nical_parse_datetime(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nical_parse_datetime", span)?;
    let text = string_arg(args, 0, "nical_parse_datetime", span)?;
    let date_only = bool_field(optional_object_arg(args, 1).as_ref(), "date_only", false);
    match parse_ical_datetime(&text, date_only) {
        Ok(dt) => {
            let mut m = HashMap::new();
            m.insert("year".into(), Value::Int(dt.year as i64).ref_cell());
            m.insert("month".into(), Value::Int(dt.month as i64).ref_cell());
            m.insert("day".into(), Value::Int(dt.day as i64).ref_cell());
            m.insert("hour".into(), Value::Int(dt.hour as i64).ref_cell());
            m.insert("minute".into(), Value::Int(dt.minute as i64).ref_cell());
            m.insert("second".into(), Value::Int(dt.second as i64).ref_cell());
            m.insert("utc".into(), Value::Bool(dt.utc).ref_cell());
            m.insert("date_only".into(), Value::Bool(dt.date_only).ref_cell());
            if let Ok(ms) = dt.to_unix_ms() {
                m.insert("unix_ms".into(), Value::Int(ms).ref_cell());
            }
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(map_ical_err(span, e)),
    }
}

// >>> nical.format_datetime(0)
// => "19700101T000000Z"
fn nical_format_datetime(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nical_format_datetime", span)?;
    let ms = match &*args[0].borrow() {
        Value::Int(n) => *n,
        other => {
            return Err(type_err(
                span,
                format!(
                    "nical_format_datetime() expects int unix_ms, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    Ok(Value::String(unix_ms_to_ical(ms)).ref_cell())
}

// >>> nical.build_calendar({events: [{summary: "Hi", uid: "1"}]}).kind
// => "calendar"
fn nical_build_calendar(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nical_build_calendar", span)?;
    let map = optional_object_arg(args, 0).unwrap_or_default();
    let mut b = calendar();
    if let Some(p) = string_field(&map, "prodid") {
        b = b.prodid(p);
    }
    if let Some(m) = string_field(&map, "method") {
        b = b.method(m);
    }
    if let Some(events) = map.get("events") {
        if let Value::Array(arr) = &*events.borrow() {
            for ev in arr {
                if let Value::Object(em) = &*ev.borrow() {
                    let comp = event_from_map(em).map_err(|msg| {
                        RuntimeError::at(span, codes::E4332_NICAL_TYPE, msg)
                    })?;
                    b = b.event(|_| EventBuilder {
                        props: comp.properties,
                        alarms: comp.children,
                    });
                }
            }
        }
    }
    Ok(component_to_niao(&b.build()).ref_cell())
}

// >>> nical.build_contact({full_name: "Ada", email: "a@e.com"}).props.FN
// => "Ada"
fn nical_build_contact(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nical_build_contact", span)?;
    let map = optional_object_arg(args, 0).unwrap_or_default();
    let mut b = contact();
    if let Some(s) = string_field(&map, "full_name") {
        b = b.full_name(s);
    }
    if let Some(s) = string_field(&map, "email") {
        b = b.email(s);
    }
    if let Some(s) = string_field(&map, "tel") {
        b = b.tel(s);
    }
    if let Some(s) = string_field(&map, "org") {
        b = b.org(s);
    }
    if let Some(s) = string_field(&map, "uid") {
        b = b.uid(s);
    }
    if let (Some(f), Some(g)) = (string_field(&map, "family"), string_field(&map, "given")) {
        b = b.structured_name(f, g);
    }
    Ok(component_to_niao(&b.build()).ref_cell())
}

// >>> nical.get({name: "VEVENT", props: {SUMMARY: "X"}}, "summary")
// => "X"
fn nical_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nical_get", span)?;
    let key = string_arg(args, 1, "nical_get", span)?.to_ascii_lowercase();
    match &*args[0].borrow() {
        Value::Object(m) => {
            if let Some(props) = m.get("props") {
                if let Value::Object(pm) = &*props.borrow() {
                    if let Some(v) = pm.get(&key) {
                        return Ok(v.clone());
                    }
                }
            }
            Ok(Value::Nil.ref_cell())
        }
        other => Err(type_err(
            span,
            format!("nical_get() expects object, got {}", other.type_name()),
        )),
    }
}

// >>> len(nical.events({kind: "calendar", name: "VCALENDAR", events: [{kind: "event"}]}))
// => 1
fn nical_events(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nical_events", span)?;
    match &*args[0].borrow() {
        Value::Object(m) => {
            if let Some(ev) = m.get("events") {
                return Ok(ev.clone());
            }
            Ok(Value::Array(Vec::new()).ref_cell())
        }
        other => Err(type_err(
            span,
            format!("nical_events() expects object, got {}", other.type_name()),
        )),
    }
}

fn nical_todos(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nical_todos", span)?;
    match &*args[0].borrow() {
        Value::Object(m) => {
            if let Some(td) = m.get("todos") {
                return Ok(td.clone());
            }
            Ok(Value::Array(Vec::new()).ref_cell())
        }
        other => Err(type_err(
            span,
            format!("nical_todos() expects object, got {}", other.type_name()),
        )),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nical_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nical_fns![
    ("nical_parse", "parse", nical_parse),
    ("nical_parse_all", "parse_all", nical_parse_all),
    ("nical_parse_calendar", "parse_calendar", nical_parse_calendar),
    ("nical_parse_contacts", "parse_contacts", nical_parse_contacts),
    ("nical_parse_file", "parse_file", nical_parse_file),
    ("nical_valid", "valid", nical_valid),
    ("nical_emit", "emit", nical_emit),
    ("nical_emit_all", "emit_all", nical_emit_all),
    ("nical_emit_file", "emit_file", nical_emit_file),
    ("nical_parse_rrule", "parse_rrule", nical_parse_rrule),
    ("nical_emit_rrule", "emit_rrule", nical_emit_rrule),
    ("nical_rrule_between", "rrule_between", nical_rrule_between),
    ("nical_parse_datetime", "parse_datetime", nical_parse_datetime),
    ("nical_format_datetime", "format_datetime", nical_format_datetime),
    ("nical_build_calendar", "build_calendar", nical_build_calendar),
    ("nical_build_contact", "build_contact", nical_build_contact),
    ("nical_get", "get", nical_get),
    ("nical_events", "events", nical_events),
    ("nical_todos", "todos", nical_todos),
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

pub const MODULE_NAME: &str = "nical";
pub const MODULE_PATHS: &[&str] = &["nical", "std/nical"];

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
    fn parse_contact() {
        let src = "BEGIN:VCARD\nVERSION:4.0\nFN:Test\nEND:VCARD\n";
        let args = [Value::String(src.into()).ref_cell()];
        let v = nical_parse_contacts(&args, span()).unwrap();
        match &*v.borrow() {
            Value::Array(a) => assert_eq!(a.len(), 1),
            other => panic!("{other:?}"),
        }
    }
}
