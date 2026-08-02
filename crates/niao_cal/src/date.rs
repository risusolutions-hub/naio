//! Civil dates backed by Howard Hinnant day numbers via `niao_time`.

use crate::error::CalError;
use niao_time::{
    civil_from_days, days_from_civil, days_in_month, is_leap_year, is_valid_date,
    weekday_from_days, MONTH_ABBR, MONTH_NAMES, WEEKDAY_ABBR, WEEKDAY_NAMES,
};

/// Map `niao_time` weekday (offset +1) to ISO Mon=0 .. Sun=6.
#[inline]
fn weekday_iso(z: i32) -> u8 {
    let w = weekday_from_days(z) as i32;
    ((w - 1).rem_euclid(7)) as u8
}

/// A validated civil calendar date (no time-of-day).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    days: i32,
}

impl Date {
    /// Construct a date from year, month, day.
    ///
    /// >>> use niao_cal::Date;
    /// >>> let d = Date::new(2026, 7, 13).unwrap();
    /// >>> assert_eq!(d.year(), 2026);
    pub fn new(year: i32, month: u32, day: u32) -> Result<Self, CalError> {
        if month == 0 || month > 12 {
            return Err(CalError::InvalidMonth(month));
        }
        if !is_valid_date(year, month, day) {
            return Err(CalError::InvalidDate { year, month, day });
        }
        Ok(Self {
            days: days_from_civil(year, month, day),
        })
    }

    #[inline]
    pub fn from_days(days: i32) -> Self {
        Self { days }
    }

    #[inline]
    pub fn to_days(self) -> i32 {
        self.days
    }

    #[inline]
    pub fn year(self) -> i32 {
        let (y, _, _) = civil_from_days(self.days);
        y
    }

    #[inline]
    pub fn month(self) -> u32 {
        let (_, m, _) = civil_from_days(self.days);
        m
    }

    #[inline]
    pub fn day(self) -> u32 {
        let (_, _, d) = civil_from_days(self.days);
        d
    }

    /// ISO weekday: Monday = 0 .. Sunday = 6.
    ///
    /// >>> use niao_cal::Date;
    /// >>> Date::new(2026, 7, 13).unwrap().weekday()
    /// 0
    #[inline]
    pub fn weekday(self) -> u8 {
        weekday_iso(self.days)
    }

    /// Day of year, 1-based.
    ///
    /// >>> use niao_cal::Date;
    /// >>> Date::new(2026, 1, 1).unwrap().ordinal()
    /// 1
    pub fn ordinal(self) -> u32 {
        let jan1 = days_from_civil(self.year(), 1, 1);
        (self.days - jan1 + 1) as u32
    }

    /// Calendar quarter 1..=4.
    pub fn quarter(self) -> u32 {
        (self.month() - 1) / 3 + 1
    }

    /// ISO calendar year, week number (1..=53), and weekday (Mon=0).
    ///
    /// >>> use niao_cal::Date;
    /// >>> let (y, w, wd) = Date::new(2026, 1, 5).unwrap().iso_week();
    /// >>> assert_eq!((y, w), (2026, 2));
    pub fn iso_week(self) -> (i32, u32, u8) {
        iso_week_from_days(self.days)
    }

    pub fn add_days(self, delta: i32) -> Self {
        Self {
            days: self.days.saturating_add(delta),
        }
    }

    pub fn format_iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year(), self.month(), self.day())
    }
}

/// Signed calendar-day difference `to - from`.
///
/// >>> use niao_cal::{Date, diff_days};
/// >>> diff_days(Date::new(2026, 7, 1).unwrap(), Date::new(2026, 7, 13).unwrap())
/// 12
#[inline]
pub fn diff_days(from: Date, to: Date) -> i32 {
    to.to_days() - from.to_days()
}

/// Parse `YYYY-MM-DD` or `YYYYMMDD`.
///
/// >>> use niao_cal::parse_date;
/// >>> parse_date("2026-07-13").unwrap().day()
/// 13
pub fn parse_date(text: &str) -> Result<Date, CalError> {
    let t = text.trim();
    if t.len() == 10 && t.as_bytes()[4] == b'-' && t.as_bytes()[7] == b'-' {
        let year: i32 = t[0..4]
            .parse()
            .map_err(|_| CalError::ParseError(format!("invalid year in '{text}'")))?;
        let month: u32 = t[5..7]
            .parse()
            .map_err(|_| CalError::ParseError(format!("invalid month in '{text}'")))?;
        let day: u32 = t[8..10]
            .parse()
            .map_err(|_| CalError::ParseError(format!("invalid day in '{text}'")))?;
        return Date::new(year, month, day);
    }
    if t.len() == 8 && t.chars().all(|c| c.is_ascii_digit()) {
        let year: i32 = t[0..4]
            .parse()
            .map_err(|_| CalError::ParseError(format!("invalid year in '{text}'")))?;
        let month: u32 = t[4..6]
            .parse()
            .map_err(|_| CalError::ParseError(format!("invalid month in '{text}'")))?;
        let day: u32 = t[6..8]
            .parse()
            .map_err(|_| CalError::ParseError(format!("invalid day in '{text}'")))?;
        return Date::new(year, month, day);
    }
    Err(CalError::ParseError(format!(
        "expected YYYY-MM-DD or YYYYMMDD, got '{text}'"
    )))
}

/// Format a date with a small strftime-like pattern.
///
/// Supported: `%Y` `%m` `%d` `%j` `%Q` `%W` `%w` `%a` `%A` `%b` `%B`
///
/// >>> use niao_cal::{Date, format_date};
/// >>> format_date(&Date::new(2026, 7, 13).unwrap(), "%Y-%m-%d")
/// "2026-07-13"
pub fn format_date(date: &Date, fmt: &str) -> String {
    if fmt == "%Y-%m-%d" {
        return date.format_iso();
    }
    let (_, iso_w, _) = date.iso_week();
    let y = date.year();
    let m = date.month();
    let d = date.day();
    let wd = date.weekday() as usize;
    let mut out = String::with_capacity(fmt.len() + 8);
    let bytes = fmt.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 1 < bytes.len() {
            match bytes[i + 1] as char {
                'Y' => out.push_str(&format!("{y:04}")),
                'm' => out.push_str(&format!("{m:02}")),
                'd' => out.push_str(&format!("{d:02}")),
                'j' => out.push_str(&format!("{:03}", date.ordinal())),
                'Q' => out.push_str(&date.quarter().to_string()),
                'W' => out.push_str(&format!("{iso_w:02}")),
                'w' => out.push_str(&(date.weekday() as u32).to_string()),
                'a' => out.push_str(WEEKDAY_ABBR[wd]),
                'A' => out.push_str(WEEKDAY_NAMES[wd]),
                'b' => out.push_str(MONTH_ABBR[(m - 1) as usize]),
                'B' => out.push_str(MONTH_NAMES[(m - 1) as usize]),
                c => {
                    out.push('%');
                    out.push(c);
                }
            }
            i += 2;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Inclusive date range as a Vec (empty when `end < start`).
///
/// >>> use niao_cal::{Date, date_range};
/// >>> date_range(Date::new(2026, 7, 1).unwrap(), Date::new(2026, 7, 3).unwrap()).len()
/// 3
pub fn date_range(start: Date, end: Date) -> Vec<Date> {
    let a = start.to_days();
    let b = end.to_days();
    if b < a {
        return Vec::new();
    }
    let len = (b - a + 1) as usize;
    let mut out = Vec::with_capacity(len);
    for days in a..=b {
        out.push(Date::from_days(days));
    }
    out
}

/// ISO week computation: ISO year, week 1..=53, weekday Mon=0.
fn iso_week_from_days(z: i32) -> (i32, u32, u8) {
    let wd = weekday_iso(z) as i32;
    let thursday = z - wd + 3;
    let (iso_year, _, _) = civil_from_days(thursday);
    let jan4 = days_from_civil(iso_year, 1, 4);
    let week1_monday = jan4 - weekday_iso(jan4) as i32;
    let week = ((thursday - week1_monday) / 7 + 1) as u32;
    (iso_year, week, wd as u8)
}

pub fn leap_year(year: i32) -> bool {
    is_leap_year(year)
}

pub fn days_in_month_of(year: i32, month: u32) -> Result<u32, CalError> {
    if month == 0 || month > 12 {
        return Err(CalError::InvalidMonth(month));
    }
    Ok(days_in_month(year, month))
}

pub fn valid_date(year: i32, month: u32, day: u32) -> bool {
    is_valid_date(year, month, day)
}

pub fn weekday_names() -> &'static [&'static str; 7] {
    &WEEKDAY_NAMES
}

pub fn month_names() -> &'static [&'static str; 12] {
    &MONTH_NAMES
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::add_business_days;

    #[test]
    fn iso_week_jan5_2026() {
        let d = Date::new(2026, 1, 5).unwrap();
        assert_eq!(d.iso_week(), (2026, 2, 0));
    }

    #[test]
    fn parse_compact() {
        let d = parse_date("20260713").unwrap();
        assert_eq!(d.format_iso(), "2026-07-13");
    }

    #[test]
    fn weekdays_july_2026() {
        let fri = Date::new(2026, 7, 10).unwrap();
        let mon = Date::new(2026, 7, 13).unwrap();
        assert_eq!(fri.weekday(), 4);
        assert_eq!(mon.weekday(), 0);
        assert_eq!(
            add_business_days(fri, 1, &crate::business::default_weekend()).format_iso(),
            "2026-07-13"
        );
    }
}
