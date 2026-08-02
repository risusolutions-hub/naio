use niao_time::WEEKDAY_NAMES;

const WEEKDAY_ABBR: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

const MONTH_ABBR: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

const MONTH_NAMES: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Next,
    Last,
    This,
    Ago,
    In,
    At,
    On,
    EndOf,
}

pub fn month_from_token(tok: &str) -> Option<u32> {
    let t = tok.trim_end_matches('.').to_ascii_lowercase();
    for (i, name) in MONTH_NAMES.iter().enumerate() {
        if t == name.to_ascii_lowercase() || t == MONTH_ABBR[i].to_ascii_lowercase() {
            return Some((i + 1) as u32);
        }
    }
    None
}

pub fn weekday_from_token(tok: &str) -> Option<usize> {
    let t = tok.trim_end_matches('.').to_ascii_lowercase();
    for (i, name) in WEEKDAY_NAMES.iter().enumerate() {
        if t == name.to_ascii_lowercase() || t == WEEKDAY_ABBR[i].to_ascii_lowercase() {
            return Some(i);
        }
    }
    None
}

pub fn unit_from_token(tok: &str) -> Option<Unit> {
    match tok.to_ascii_lowercase().as_str() {
        "s" | "sec" | "secs" | "second" | "seconds" => Some(Unit::Second),
        "m" | "min" | "mins" | "minute" | "minutes" => Some(Unit::Minute),
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(Unit::Hour),
        "d" | "day" | "days" => Some(Unit::Day),
        "w" | "wk" | "wks" | "week" | "weeks" => Some(Unit::Week),
        "mo" | "mos" | "mon" | "mons" | "month" | "months" => Some(Unit::Month),
        "y" | "yr" | "yrs" | "year" | "years" => Some(Unit::Year),
        _ => None,
    }
}

pub fn modifier_from_token(tok: &str) -> Option<Modifier> {
    match tok.to_ascii_lowercase().as_str() {
        "next" => Some(Modifier::Next),
        "last" | "previous" | "prev" => Some(Modifier::Last),
        "this" => Some(Modifier::This),
        "ago" => Some(Modifier::Ago),
        "in" => Some(Modifier::In),
        "at" => Some(Modifier::At),
        "on" => Some(Modifier::On),
        "end" => Some(Modifier::EndOf),
        _ => None,
    }
}

pub fn is_now(tok: &str) -> bool {
    matches!(tok.to_ascii_lowercase().as_str(), "now" | "right now")
}

pub fn is_today(tok: &str) -> bool {
    matches!(
        tok.to_ascii_lowercase().as_str(),
        "today" | "tonight" | "this evening"
    )
}

pub fn is_tomorrow(tok: &str) -> bool {
    tok.eq_ignore_ascii_case("tomorrow")
}

pub fn is_yesterday(tok: &str) -> bool {
    tok.eq_ignore_ascii_case("yesterday")
}

pub fn is_noon(tok: &str) -> bool {
    matches!(tok.to_ascii_lowercase().as_str(), "noon" | "midday")
}

pub fn is_midnight(tok: &str) -> bool {
    matches!(tok.to_ascii_lowercase().as_str(), "midnight")
}

pub fn is_time_word(tok: &str) -> Option<(u32, u32)> {
    let t = tok.to_ascii_lowercase();
    if let Some(rest) = t.strip_suffix("am").or_else(|| t.strip_suffix("a.m.")) {
        return parse_hour_min(rest, false);
    }
    if let Some(rest) = t.strip_suffix("pm").or_else(|| t.strip_suffix("p.m.")) {
        return parse_hour_min(rest, true);
    }
    None
}

fn parse_hour_min(s: &str, pm: bool) -> Option<(u32, u32)> {
    let s = s.trim();
    let (h, m) = if let Some((a, b)) = s.split_once(':') {
        (a.trim().parse().ok()?, b.trim().parse().ok()?)
    } else if s.is_empty() {
        return None;
    } else {
        (s.parse().ok()?, 0)
    };
    if h > 12 || m >= 60 {
        return None;
    }
    let hour = if pm {
        if h == 12 {
            12
        } else {
            h + 12
        }
    } else if h == 12 {
        0
    } else {
        h
    };
    Some((hour, m))
}

pub fn supported_languages() -> &'static [&'static str] {
    &["en"]
}

// >>> use niao_when::supported_languages;
// >>> supported_languages().contains(&"en")
// true
