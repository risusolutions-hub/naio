use crate::datetime::parse_ical_datetime;
use crate::error::IcalError;
use std::collections::HashMap;

/// Parsed RRULE fields (RFC 5545 subset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RRule {
    pub freq: Frequency,
    pub interval: u32,
    pub count: Option<u32>,
    pub until: Option<String>,
    pub bysecond: Vec<u8>,
    pub byminute: Vec<u8>,
    pub byhour: Vec<u8>,
    pub byday: Vec<ByDay>,
    pub bymonthday: Vec<i8>,
    pub byyearday: Vec<i16>,
    pub byweekno: Vec<i8>,
    pub bymonth: Vec<u8>,
    pub bysetpos: Vec<i8>,
    pub wkst: Weekday,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frequency {
    Secondly,
    Minutely,
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weekday {
    Mo,
    Tu,
    We,
    Th,
    Fr,
    Sa,
    Su,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByDay {
    pub ord: Option<i8>,
    pub weekday: Weekday,
}

impl Default for RRule {
    fn default() -> Self {
        Self {
            freq: Frequency::Daily,
            interval: 1,
            count: None,
            until: None,
            bysecond: Vec::new(),
            byminute: Vec::new(),
            byhour: Vec::new(),
            byday: Vec::new(),
            bymonthday: Vec::new(),
            byyearday: Vec::new(),
            byweekno: Vec::new(),
            bymonth: Vec::new(),
            bysetpos: Vec::new(),
            wkst: Weekday::Mo,
        }
    }
}

/// Parse an RRULE string (`FREQ=…` with optional `RRULE:` prefix).
///
/// >>> use niao_ical::rrule::parse_rrule;
/// >>> let r = parse_rrule("FREQ=WEEKLY;BYDAY=MO,WE;COUNT=10").unwrap();
/// >>> r.freq == niao_ical::rrule::Frequency::Weekly && r.count == Some(10)
/// true
pub fn parse_rrule(raw: &str) -> Result<RRule, IcalError> {
    let s = raw.trim();
    let body = s
        .strip_prefix("RRULE:")
        .or_else(|| s.strip_prefix("rrule:"))
        .unwrap_or(s);
    let mut rule = RRule::default();
    let mut parts = HashMap::new();
    for piece in body.split(';') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        let (k, v) = piece
            .split_once('=')
            .ok_or_else(|| IcalError::InvalidRrule(format!("expected KEY=VAL, got {piece}")))?;
        parts.insert(k.trim().to_ascii_uppercase(), v.trim().to_string());
    }
    let freq = parts
        .get("FREQ")
        .ok_or_else(|| IcalError::InvalidRrule("FREQ required".into()))?;
    rule.freq = parse_freq(freq)?;
    rule.interval = parts
        .get("INTERVAL")
        .map(|s| {
            s.parse()
                .map_err(|_| IcalError::InvalidRrule("bad INTERVAL".into()))
        })
        .transpose()?
        .unwrap_or(1);
    rule.count = parts
        .get("COUNT")
        .map(|s| {
            s.parse()
                .map_err(|_| IcalError::InvalidRrule("bad COUNT".into()))
        })
        .transpose()?;
    rule.until = parts.get("UNTIL").cloned();
    if let Some(w) = parts.get("WKST") {
        rule.wkst = parse_weekday_token(w)?;
    }
    if let Some(v) = parts.get("BYDAY") {
        rule.byday = parse_byday_list(v)?;
    }
    if let Some(v) = parts.get("BYMONTHDAY") {
        rule.bymonthday = parse_i8_list(v)?;
    }
    if let Some(v) = parts.get("BYMONTH") {
        rule.bymonth = parse_u8_list(v)?;
    }
    if let Some(v) = parts.get("BYHOUR") {
        rule.byhour = parse_u8_list(v)?;
    }
    if let Some(v) = parts.get("BYMINUTE") {
        rule.byminute = parse_u8_list(v)?;
    }
    if let Some(v) = parts.get("BYSECOND") {
        rule.bysecond = parse_u8_list(v)?;
    }
    if let Some(v) = parts.get("BYYEARDAY") {
        rule.byyearday = v
            .split(',')
            .map(|s| {
                s.parse()
                    .map_err(|_| IcalError::InvalidRrule("bad BYYEARDAY".into()))
            })
            .collect::<Result<_, _>>()?;
    }
    if let Some(v) = parts.get("BYWEEKNO") {
        rule.byweekno = parse_i8_list(v)?;
    }
    if let Some(v) = parts.get("BYSETPOS") {
        rule.bysetpos = parse_i8_list(v)?;
    }
    Ok(rule)
}

/// Serialize RRULE to `FREQ=…` form (no `RRULE:` prefix).
pub fn emit_rrule(rule: &RRule) -> String {
    let mut out = format!(
        "FREQ={};INTERVAL={}",
        freq_str(rule.freq),
        rule.interval.max(1)
    );
    if let Some(c) = rule.count {
        out.push_str(&format!(";COUNT={c}"));
    }
    if let Some(u) = &rule.until {
        out.push_str(&format!(";UNTIL={u}"));
    }
    if !rule.byday.is_empty() {
        out.push_str(";BYDAY=");
        for (i, bd) in rule.byday.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            if let Some(o) = bd.ord {
                out.push_str(&o.to_string());
            }
            out.push_str(weekday_str(bd.weekday));
        }
    }
    if !rule.bymonthday.is_empty() {
        out.push_str(";BYMONTHDAY=");
        join_nums(&mut out, &rule.bymonthday);
    }
    if !rule.bymonth.is_empty() {
        out.push_str(";BYMONTH=");
        join_nums(&mut out, &rule.bymonth);
    }
    if rule.wkst != Weekday::Mo {
        out.push_str(&format!(";WKST={}", weekday_str(rule.wkst)));
    }
    out
}

/// Expand occurrences between `dtstart` (iCal DATE-TIME) and optional `until_ms` / `count`.
///
/// Returns UTC unix milliseconds for each occurrence (inclusive of dtstart when it matches).
pub fn rrule_occurrences(
    rule: &RRule,
    dtstart: &str,
    after_ms: Option<i64>,
    before_ms: Option<i64>,
    max_count: Option<usize>,
) -> Result<Vec<i64>, IcalError> {
    use chrono::{TimeZone, Utc};
    use rrule::{RRuleError, RRuleSet, Tz};

    let _ = parse_ical_datetime(dtstart, false)?;
    let body = emit_rrule(rule);
    let set_src = format!("DTSTART:{dtstart}\nRRULE:{body}");
    let mut set: RRuleSet = set_src
        .parse::<RRuleSet>()
        .map_err(|e: RRuleError| IcalError::InvalidRrule(e.to_string()))?;

    if let Some(ms) = after_ms {
        if let Some(dt) = Utc.timestamp_millis_opt(ms).single() {
            set = set.after(dt.with_timezone(&Tz::UTC));
        }
    }
    if let Some(ms) = before_ms {
        if let Some(dt) = Utc.timestamp_millis_opt(ms).single() {
            set = set.before(dt.with_timezone(&Tz::UTC));
        }
    }

    let limit = max_count
        .or(rule.count.map(|c| c as usize))
        .unwrap_or(500)
        .min(65_535) as u16;
    let result = set.all(limit);
    Ok(result
        .dates
        .into_iter()
        .map(|d| d.timestamp_millis())
        .collect())
}

fn parse_freq(s: &str) -> Result<Frequency, IcalError> {
    match s.to_ascii_uppercase().as_str() {
        "SECONDLY" => Ok(Frequency::Secondly),
        "MINUTELY" => Ok(Frequency::Minutely),
        "HOURLY" => Ok(Frequency::Hourly),
        "DAILY" => Ok(Frequency::Daily),
        "WEEKLY" => Ok(Frequency::Weekly),
        "MONTHLY" => Ok(Frequency::Monthly),
        "YEARLY" => Ok(Frequency::Yearly),
        other => Err(IcalError::InvalidRrule(format!("unknown FREQ {other}"))),
    }
}

fn freq_str(f: Frequency) -> &'static str {
    match f {
        Frequency::Secondly => "SECONDLY",
        Frequency::Minutely => "MINUTELY",
        Frequency::Hourly => "HOURLY",
        Frequency::Daily => "DAILY",
        Frequency::Weekly => "WEEKLY",
        Frequency::Monthly => "MONTHLY",
        Frequency::Yearly => "YEARLY",
    }
}

fn parse_weekday_token(s: &str) -> Result<Weekday, IcalError> {
    match s.to_ascii_uppercase().as_str() {
        "MO" => Ok(Weekday::Mo),
        "TU" => Ok(Weekday::Tu),
        "WE" => Ok(Weekday::We),
        "TH" => Ok(Weekday::Th),
        "FR" => Ok(Weekday::Fr),
        "SA" => Ok(Weekday::Sa),
        "SU" => Ok(Weekday::Su),
        other => Err(IcalError::InvalidRrule(format!("bad weekday {other}"))),
    }
}

fn weekday_str(w: Weekday) -> &'static str {
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

fn parse_byday_list(s: &str) -> Result<Vec<ByDay>, IcalError> {
    s.split(',')
        .map(|tok| {
            let tok = tok.trim();
            let (ord, wd) = if tok.len() >= 3
                && tok[..tok.len() - 2]
                    .chars()
                    .all(|c| c == '-' || c.is_ascii_digit())
            {
                let ord: i8 = tok[..tok.len() - 2]
                    .parse()
                    .map_err(|_| IcalError::InvalidRrule("bad BYDAY ord".into()))?;
                (Some(ord), &tok[tok.len() - 2..])
            } else {
                (None, tok)
            };
            Ok(ByDay {
                ord,
                weekday: parse_weekday_token(wd)?,
            })
        })
        .collect()
}

fn parse_u8_list(s: &str) -> Result<Vec<u8>, IcalError> {
    s.split(',')
        .map(|x| {
            x.parse()
                .map_err(|_| IcalError::InvalidRrule("bad integer list".into()))
        })
        .collect()
}

fn parse_i8_list(s: &str) -> Result<Vec<i8>, IcalError> {
    s.split(',')
        .map(|x| {
            x.parse()
                .map_err(|_| IcalError::InvalidRrule("bad integer list".into()))
        })
        .collect()
}

fn join_nums<T: std::fmt::Display>(out: &mut String, vals: &[T]) {
    for (i, v) in vals.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&v.to_string());
    }
}

/// Convert RRULE to a Niao-friendly map.
pub fn rrule_to_map(rule: &RRule) -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("freq".into(), freq_str(rule.freq).to_ascii_lowercase());
    m.insert("interval".into(), rule.interval.to_string());
    if let Some(c) = rule.count {
        m.insert("count".into(), c.to_string());
    }
    if let Some(u) = &rule.until {
        m.insert("until".into(), u.clone());
    }
    if !rule.byday.is_empty() {
        let days: Vec<String> = rule
            .byday
            .iter()
            .map(|bd| {
                let mut s = String::new();
                if let Some(o) = bd.ord {
                    s.push_str(&o.to_string());
                }
                s.push_str(weekday_str(bd.weekday));
                s
            })
            .collect();
        m.insert("byday".into(), days.join(","));
    }
    m
}

/// Build RRULE from a map (inverse of `rrule_to_map`).
pub fn rrule_from_map(m: &HashMap<String, String>) -> Result<RRule, IcalError> {
    let freq = m
        .get("freq")
        .ok_or_else(|| IcalError::InvalidRrule("freq required".into()))?;
    let mut rule = RRule {
        freq: parse_freq(&freq.to_ascii_uppercase())?,
        interval: m.get("interval").and_then(|s| s.parse().ok()).unwrap_or(1),
        count: m.get("count").and_then(|s| s.parse().ok()),
        until: m.get("until").cloned(),
        ..RRule::default()
    };
    if let Some(d) = m.get("byday") {
        rule.byday = parse_byday_list(d)?;
    }
    Ok(rule)
}
