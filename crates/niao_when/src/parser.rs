use crate::error::WhenError;
use crate::lexicon::{
    is_midnight, is_noon, is_now, is_time_word, is_today, is_tomorrow, is_yesterday,
    modifier_from_token, month_from_token, unit_from_token, weekday_from_token, Modifier, Unit,
};
use crate::options::{DateOrder, ParseOptions, PreferDirection, RequireParts};
use niao_parallel::map as par_map;
use niao_time::{
    civil_from_days, civil_to_ms, days_from_civil, days_in_month, is_valid_date, ms_to_civil,
    parse_rfc2822, parse_rfc3339, weekday_from_days, CivilDateTime, Timezone,
};

/// Parsed datetime with metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDate {
    pub unix_ms: i64,
    pub matched: String,
    pub has_date: bool,
    pub has_time: bool,
}

/// >>> use niao_when::{parse, options::ParseOptions};
/// >>> let o = ParseOptions::default().with_base_ms(1_704_067_200_000).with_timezone("UTC");
/// >>> let d = parse("next friday 5pm", &o).unwrap();
/// >>> d.has_date && d.has_time
/// true
pub fn parse(text: &str, opts: &ParseOptions) -> Result<ParsedDate, WhenError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(WhenError::Empty);
    }
    let tz = opts.resolve_tz().map_err(|e| WhenError::InvalidDate(e))?;
    let base = ms_to_civil(opts.base_ms, &tz).map_err(|e| WhenError::InvalidDate(e))?;

    if let Some(hit) = try_iso(trimmed, &tz, opts)? {
        return Ok(hit);
    }

    let norm = normalize(trimmed, opts.fuzzy);
    let tokens = tokenize(&norm);
    if tokens.is_empty() {
        return Err(WhenError::NoDate);
    }

    let mut candidates = Vec::new();
    if let Some(c) = parse_tokens(&tokens, &base, opts, trimmed)? {
        candidates.push(c);
    }
    if candidates.is_empty() {
        return Err(WhenError::NoDate);
    }
    let best = pick_best(candidates, opts)?;
    finalize(best, &tz, opts, trimmed)
}

/// >>> use niao_when::{valid, options::ParseOptions};
/// >>> valid("tomorrow at noon", &ParseOptions::default())
/// true
pub fn valid(text: &str, opts: &ParseOptions) -> bool {
    parse(text, opts).is_ok()
}

/// Return ranked parse candidates (best first).
///
/// >>> use niao_when::{parse_many, options::ParseOptions};
/// >>> let hits = parse_many("03/04/2024", &ParseOptions::default()).unwrap();
/// >>> !hits.is_empty()
/// true
pub fn parse_many(text: &str, opts: &ParseOptions) -> Result<Vec<ParsedDate>, WhenError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(WhenError::Empty);
    }
    let tz = opts.resolve_tz().map_err(|e| WhenError::InvalidDate(e))?;
    let base = ms_to_civil(opts.base_ms, &tz).map_err(|e| WhenError::InvalidDate(e))?;

    let mut out = Vec::new();
    if let Some(hit) = try_iso(trimmed, &tz, opts)? {
        out.push(hit);
    }
    let norm = normalize(trimmed, opts.fuzzy);
    let tokens = tokenize(&norm);
    if !tokens.is_empty() {
        if let Some(c) = parse_tokens(&tokens, &base, opts, trimmed)? {
            if let Ok(p) = finalize(c, &tz, opts, trimmed) {
                out.push(p);
            }
        }
        // Alternate date orders for ambiguous numeric dates.
        if looks_numeric_date(&tokens) {
            for order in [DateOrder::Mdy, DateOrder::Dmy, DateOrder::Ymd] {
                if order == opts.date_order {
                    continue;
                }
                let mut alt = opts.clone();
                alt.date_order = order;
                if let Some(c) = parse_tokens(&tokens, &base, &alt, trimmed)? {
                    if let Ok(p) = finalize(c, &tz, &alt, trimmed) {
                        if !out.iter().any(|x| x.unix_ms == p.unix_ms) {
                            out.push(p);
                        }
                    }
                }
            }
        }
    }
    if out.is_empty() {
        return Err(WhenError::NoDate);
    }
    out.sort_by_key(|a| a.unix_ms);
    Ok(out)
}

/// Parallel parse over many strings (preserves order).
///
/// >>> use niao_when::{batch_parse, options::ParseOptions};
/// >>> let texts = vec!["today", "tomorrow", "yesterday"];
/// >>> let out = batch_parse(&texts, &ParseOptions::default(), 2);
/// >>> out.len() == 3
/// true
pub fn batch_parse(
    texts: &[String],
    opts: &ParseOptions,
    threads: usize,
) -> Vec<Result<ParsedDate, WhenError>> {
    par_map(texts, threads, |t| parse(t, opts))
}

#[derive(Debug, Clone)]
struct Candidate {
    civil: CivilDateTime,
    has_date: bool,
    has_time: bool,
    score: i32,
}

fn finalize(
    cand: Candidate,
    tz: &Timezone,
    opts: &ParseOptions,
    matched: &str,
) -> Result<ParsedDate, WhenError> {
    match opts.require {
        RequireParts::Date if !cand.has_date => return Err(WhenError::NoDate),
        RequireParts::Time if !cand.has_time => {
            return Err(WhenError::InvalidTime("time required".into()))
        }
        RequireParts::Both if !(cand.has_date && cand.has_time) => {
            return Err(WhenError::Ambiguous("date and time required".into()));
        }
        _ => {}
    }
    let ms = civil_to_ms(&cand.civil, tz).map_err(|e| WhenError::InvalidDate(e))?;
    Ok(ParsedDate {
        unix_ms: ms,
        matched: matched.to_string(),
        has_date: cand.has_date,
        has_time: cand.has_time,
    })
}

fn pick_best(mut cands: Vec<Candidate>, opts: &ParseOptions) -> Result<Candidate, WhenError> {
    cands.sort_by(|a, b| b.score.cmp(&a.score));
    let best = cands.into_iter().next().unwrap();
    if !best.has_date && !best.has_time {
        return Err(WhenError::NoDate);
    }
    // Disambiguate weekday-only style using prefer direction.
    if best.has_date {
        let tz = opts.resolve_tz().map_err(|e| WhenError::InvalidDate(e))?;
        let ms = civil_to_ms(&best.civil, &tz).map_err(|e| WhenError::InvalidDate(e))?;
        let adjusted = apply_prefer(ms, opts)?;
        if adjusted != ms {
            let civil = ms_to_civil(adjusted, &tz).map_err(|e| WhenError::InvalidDate(e))?;
            return Ok(Candidate {
                civil,
                has_date: best.has_date,
                has_time: best.has_time,
                score: best.score,
            });
        }
    }
    Ok(best)
}

fn apply_prefer(ms: i64, opts: &ParseOptions) -> Result<i64, WhenError> {
    match opts.prefer {
        PreferDirection::Current => Ok(ms),
        PreferDirection::Future if ms < opts.base_ms => Ok(ms + 7 * 86_400_000),
        PreferDirection::Past if ms > opts.base_ms => Ok(ms - 7 * 86_400_000),
        _ => Ok(ms),
    }
}

fn try_iso(
    text: &str,
    tz: &Timezone,
    opts: &ParseOptions,
) -> Result<Option<ParsedDate>, WhenError> {
    if text.len() >= 20 && (text.contains('T') || text.contains('t')) {
        if let Ok((civil, _off)) = parse_rfc3339(text) {
            let ms = civil_to_ms(&civil, tz).map_err(|e| WhenError::InvalidDate(e))?;
            return Ok(Some(ParsedDate {
                unix_ms: ms,
                matched: text.to_string(),
                has_date: true,
                has_time: true,
            }));
        }
    }
    if text.len() >= 10 && text.as_bytes()[4] == b'-' {
        if let Ok(civil) = niao_time::parse_datetime(text, "%Y-%m-%d") {
            let mut c = civil;
            c.hour = 0;
            c.minute = 0;
            c.second = 0;
            let ms = civil_to_ms(&c, tz).map_err(|e| WhenError::InvalidDate(e))?;
            return Ok(Some(ParsedDate {
                unix_ms: ms,
                matched: text.to_string(),
                has_date: true,
                has_time: false,
            }));
        }
    }
    if let Ok(civil) = parse_rfc2822(text) {
        let ms = civil_to_ms(&civil, tz).map_err(|e| WhenError::InvalidDate(e))?;
        return Ok(Some(ParsedDate {
            unix_ms: ms,
            matched: text.to_string(),
            has_date: true,
            has_time: true,
        }));
    }
    let _ = opts;
    Ok(None)
}

fn normalize(s: &str, fuzzy: bool) -> String {
    let lower = s.to_ascii_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut prev_space = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() || ch == ':' || ch == '-' || ch == '/' || ch == '.' {
            out.push(ch);
            prev_space = false;
        } else if ch.is_whitespace() || (fuzzy && (ch == ',' || ch == ';')) {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        }
    }
    out.trim().to_string()
}

fn tokenize(s: &str) -> Vec<String> {
    s.split_whitespace().map(|t| t.to_string()).collect()
}

fn looks_numeric_date(tokens: &[String]) -> bool {
    tokens.iter().any(|t| t.contains('/') || t.contains('-'))
}

fn parse_tokens(
    tokens: &[String],
    base: &CivilDateTime,
    opts: &ParseOptions,
    raw: &str,
) -> Result<Option<Candidate>, WhenError> {
    let mut civil = *base;
    let mut has_date = false;
    let mut has_time = false;
    let mut score = 0i32;
    let mut i = 0usize;

    // Leading "in" relative: in 2 weeks
    if i < tokens.len() {
        if let Some(Modifier::In) = modifier_from_token(&tokens[i]) {
            if i + 2 < tokens.len() {
                if let Some(n) = parse_number(&tokens[i + 1]) {
                    if let Some(unit) = unit_from_token(&tokens[i + 2]) {
                        civil = add_relative(base, n, unit, false)?;
                        has_date = true;
                        has_time =
                            unit == Unit::Hour || unit == Unit::Minute || unit == Unit::Second;
                        score += 20;
                        i += 3;
                    }
                } else if tokens[i + 1] == "a" || tokens[i + 1] == "an" {
                    if let Some(unit) = unit_from_token(&tokens[i + 2]) {
                        civil = add_relative(base, 1, unit, false)?;
                        has_date = true;
                        score += 18;
                        i += 3;
                    }
                }
            }
        }
    }

    while i < tokens.len() {
        let tok = &tokens[i];

        if is_now(tok) {
            civil = *base;
            has_date = true;
            has_time = true;
            score += 30;
            i += 1;
            continue;
        }
        if is_today(tok) {
            civil.year = base.year;
            civil.month = base.month;
            civil.day = base.day;
            has_date = true;
            score += 25;
            i += 1;
            continue;
        }
        if is_tomorrow(tok) {
            civil = add_days(base, 1)?;
            has_date = true;
            score += 25;
            i += 1;
            continue;
        }
        if is_yesterday(tok) {
            civil = add_days(base, -1)?;
            has_date = true;
            score += 25;
            i += 1;
            continue;
        }

        if let Some(modifier) = modifier_from_token(tok) {
            match modifier {
                Modifier::Next | Modifier::Last | Modifier::This => {
                    if i + 1 < tokens.len() {
                        if let Some(wd) = weekday_from_token(&tokens[i + 1]) {
                            civil = nth_weekday(base, wd, modifier)?;
                            has_date = true;
                            score += 22;
                            i += 2;
                            continue;
                        }
                        if let Some(unit) = unit_from_token(&tokens[i + 1]) {
                            let n = match modifier {
                                Modifier::Next => 1,
                                Modifier::Last => -1,
                                Modifier::This => 0,
                                _ => 0,
                            };
                            civil = add_relative(base, n, unit, false)?;
                            has_date = true;
                            score += 20;
                            i += 2;
                            continue;
                        }
                    }
                }
                Modifier::EndOf => {
                    if i + 2 < tokens.len() && tokens[i + 1] == "of" {
                        if unit_from_token(&tokens[i + 2]) == Some(Unit::Month) {
                            civil = end_of_month(base)?;
                            has_date = true;
                            score += 21;
                            i += 3;
                            continue;
                        }
                    }
                }
                Modifier::At | Modifier::On | Modifier::In | Modifier::Ago => {}
            }
        }

        if let Some(month) = month_from_token(tok) {
            if i + 1 < tokens.len() {
                if let Some(day) = parse_number(&tokens[i + 1]) {
                    let year = if i + 2 < tokens.len() {
                        if let Some(y) = parse_number(&tokens[i + 2]) {
                            if y > 31 {
                                i += 3;
                                Some(y as i32)
                            } else {
                                i += 2;
                                None
                            }
                        } else {
                            i += 2;
                            None
                        }
                    } else {
                        i += 2;
                        None
                    };
                    civil = date_parts(
                        year.unwrap_or(base.year),
                        month,
                        day as u32,
                        civil.hour,
                        civil.minute,
                        civil.second,
                    )?;
                    has_date = true;
                    score += 24;
                    continue;
                }
            }
        }

        if let Some((y, m, d)) = parse_numeric_date(tok, opts.date_order, base.year) {
            civil.year = y;
            civil.month = m;
            civil.day = d;
            has_date = true;
            score += 23;
            i += 1;
            continue;
        }

        if let Some(n) = parse_number(tok) {
            if i + 1 < tokens.len() {
                if let Some(unit) = unit_from_token(&tokens[i + 1]) {
                    let ago = i + 2 <= tokens.len()
                        && tokens.get(i + 2).map(|t| t == "ago").unwrap_or(false);
                    civil = add_relative(base, if ago { -n } else { n }, unit, ago)?;
                    has_date = true;
                    has_time = matches!(unit, Unit::Hour | Unit::Minute | Unit::Second);
                    score += 19;
                    i += if ago { 3 } else { 2 };
                    continue;
                }
            }
        }

        if is_noon(tok) {
            civil.hour = 12;
            civil.minute = 0;
            civil.second = 0;
            has_time = true;
            score += 15;
            i += 1;
            continue;
        }
        if is_midnight(tok) {
            civil.hour = 0;
            civil.minute = 0;
            civil.second = 0;
            has_time = true;
            score += 15;
            i += 1;
            continue;
        }
        if let Some((h, m)) = is_time_word(tok) {
            civil.hour = h;
            civil.minute = m;
            civil.second = 0;
            has_time = true;
            score += 16;
            i += 1;
            continue;
        }
        if let Some((h, m, s)) = parse_24h(tok) {
            civil.hour = h;
            civil.minute = m;
            civil.second = s;
            has_time = true;
            score += 16;
            i += 1;
            continue;
        }

        i += 1;
    }

    if !has_date && !has_time {
        return Ok(None);
    }

    if has_time && !has_date {
        civil.year = base.year;
        civil.month = base.month;
        civil.day = base.day;
    }

    let _ = raw;
    Ok(Some(Candidate {
        civil,
        has_date,
        has_time,
        score,
    }))
}

fn parse_number(tok: &str) -> Option<i64> {
    tok.parse().ok()
}

fn parse_24h(tok: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = tok.split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let h: u32 = parts[0].parse().ok()?;
    let m: u32 = if parts.len() > 1 {
        parts[1].parse().ok()?
    } else {
        0
    };
    let s: u32 = if parts.len() > 2 {
        parts[2].parse().ok()?
    } else {
        0
    };
    if h >= 24 || m >= 60 || s >= 60 {
        return None;
    }
    Some((h, m, s))
}

fn parse_numeric_date(tok: &str, order: DateOrder, default_year: i32) -> Option<(i32, u32, u32)> {
    let sep = if tok.contains('/') {
        '/'
    } else if tok.contains('-') {
        '-'
    } else {
        return None;
    };
    let parts: Vec<&str> = tok.split(sep).collect();
    if parts.len() != 3 {
        return None;
    }
    let a: i32 = parts[0].parse().ok()?;
    let b: i32 = parts[1].parse().ok()?;
    let c: i32 = parts[2].parse().ok()?;
    let (y, m, d) = match order {
        DateOrder::Ymd => (a, b as u32, c as u32),
        DateOrder::Dmy => {
            if a > 31 {
                (a, b as u32, c as u32)
            } else {
                (
                    if c < 100 {
                        default_year / 100 * 100 + c
                    } else {
                        c
                    },
                    b as u32,
                    a as u32,
                )
            }
        }
        DateOrder::Mdy => {
            if a > 31 {
                (a, b as u32, c as u32)
            } else {
                (
                    if c < 100 {
                        default_year / 100 * 100 + c
                    } else {
                        c
                    },
                    a as u32,
                    b as u32,
                )
            }
        }
    };
    if is_valid_date(y, m, d) {
        Some((y, m, d))
    } else {
        None
    }
}

fn date_parts(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> Result<CivilDateTime, WhenError> {
    if !is_valid_date(y, m, d) {
        return Err(WhenError::InvalidDate(format!(
            "invalid date {y}-{m:02}-{d:02}"
        )));
    }
    Ok(CivilDateTime {
        year: y,
        month: m,
        day: d,
        hour: h,
        minute: mi,
        second: s,
        millisecond: 0,
    })
}

fn add_days(base: &CivilDateTime, delta: i32) -> Result<CivilDateTime, WhenError> {
    let z = days_from_civil(base.year, base.month, base.day) + delta;
    let (y, m, d) = civil_from_days(z);
    Ok(CivilDateTime {
        year: y,
        month: m,
        day: d,
        hour: base.hour,
        minute: base.minute,
        second: base.second,
        millisecond: base.millisecond,
    })
}

fn add_relative(
    base: &CivilDateTime,
    n: i64,
    unit: Unit,
    _ago: bool,
) -> Result<CivilDateTime, WhenError> {
    match unit {
        Unit::Second => shift_seconds(base, n),
        Unit::Minute => shift_seconds(base, n * 60),
        Unit::Hour => shift_seconds(base, n * 3600),
        Unit::Day => add_days(base, n as i32),
        Unit::Week => add_days(base, (n * 7) as i32),
        Unit::Month => add_months(base, n as i32),
        Unit::Year => add_months(base, (n * 12) as i32),
    }
}

fn shift_seconds(base: &CivilDateTime, secs: i64) -> Result<CivilDateTime, WhenError> {
    let day_secs = base.hour as i64 * 3600 + base.minute as i64 * 60 + base.second as i64 + secs;
    let day_delta = day_secs.div_euclid(86_400);
    let sod = day_secs.rem_euclid(86_400);
    let mut c = add_days(base, day_delta as i32)?;
    c.hour = (sod / 3600) as u32;
    c.minute = ((sod % 3600) / 60) as u32;
    c.second = (sod % 60) as u32;
    Ok(c)
}

fn add_months(base: &CivilDateTime, delta: i32) -> Result<CivilDateTime, WhenError> {
    let mut y = base.year;
    let mut m = base.month as i32 + delta;
    while m > 12 {
        m -= 12;
        y += 1;
    }
    while m < 1 {
        m += 12;
        y -= 1;
    }
    let m = m as u32;
    let dim = days_in_month(y, m);
    let d = base.day.min(dim);
    date_parts(y, m, d, base.hour, base.minute, base.second)
}

fn nth_weekday(
    base: &CivilDateTime,
    target: usize,
    modifier: Modifier,
) -> Result<CivilDateTime, WhenError> {
    let base_wd = weekday_from_days(days_from_civil(base.year, base.month, base.day));
    let mut delta = (target as i32 + 7 - base_wd as i32) % 7;
    match modifier {
        Modifier::Next => {
            if delta == 0 {
                delta = 7;
            }
        }
        Modifier::Last => {
            delta -= 7;
            if delta == 0 {
                delta = -7;
            }
        }
        Modifier::This => {}
        _ => {}
    }
    add_days(base, delta)
}

fn end_of_month(base: &CivilDateTime) -> Result<CivilDateTime, WhenError> {
    let dim = days_in_month(base.year, base.month);
    date_parts(base.year, base.month, dim, 23, 59, 59)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::ParseOptions;

    fn opts_at(y: i32, m: u32, d: u32) -> ParseOptions {
        let civil = CivilDateTime {
            year: y,
            month: m,
            day: d,
            hour: 12,
            minute: 0,
            second: 0,
            millisecond: 0,
        };
        let tz = Timezone::utc();
        let ms = civil_to_ms(&civil, &tz).unwrap();
        ParseOptions::default()
            .with_base_ms(ms)
            .with_timezone("UTC")
    }

    #[test]
    fn parse_tomorrow_noon() {
        let o = opts_at(2024, 3, 15);
        let d = parse("tomorrow at noon", &o).unwrap();
        assert!(d.has_date && d.has_time);
    }

    #[test]
    fn parse_in_two_weeks() {
        let o = opts_at(2024, 3, 15);
        let d = parse("in 2 weeks", &o).unwrap();
        assert!(d.has_date);
    }

    #[test]
    fn parse_next_friday() {
        let o = opts_at(2024, 3, 15); // Friday
        let d = parse("next friday", &o).unwrap();
        assert!(d.has_date);
    }
}
