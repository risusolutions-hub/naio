//! Zero-dependency civil date/time utilities for Niao.

mod civil;
mod format;
mod local;
mod parse;
mod tz;
pub mod unix;

pub use civil::{
    civil_from_days, days_from_civil, days_in_month, is_leap_year, is_valid_date,
    weekday_from_days, CivilDateTime, MONTH_ABBR, MONTH_NAMES, WEEKDAY_ABBR, WEEKDAY_NAMES,
};
pub use format::format_datetime;
pub use parse::{parse_datetime, parse_rfc2822, parse_rfc3339};
pub use tz::{list_timezones, resolve_timezone, Timezone, TzKind};
pub use unix::{civil_to_ms, ms_to_civil, ms_to_utc_parts, utc_parts_to_ms, UtcParts};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub unix_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Duration {
    pub millis: i64,
}

impl Duration {
    pub const fn from_millis(millis: i64) -> Self {
        Self { millis }
    }
}

impl DateTime {
    pub fn from_unix_ms(ms: i64) -> Self {
        Self { unix_ms: ms }
    }

    pub fn unix_ms(&self) -> i64 {
        self.unix_ms
    }

    pub fn to_civil(&self, tz: &Timezone) -> Result<CivilDateTime, String> {
        unix::ms_to_civil(self.unix_ms, tz)
    }

    pub fn format(&self, fmt: &str, tz: &Timezone) -> Result<String, String> {
        let civil = self.to_civil(tz)?;
        let offset = tz.offset_at_ms(self.unix_ms)?;
        format_datetime(&civil, fmt, offset)
    }

    pub fn parse(text: &str, fmt: &str, tz: &Timezone) -> Result<Self, String> {
        let civil = parse_datetime(text, fmt)?;
        let ms = unix::civil_to_ms(&civil, tz)?;
        Ok(Self::from_unix_ms(ms))
    }
}

pub fn now_unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn to_rfc3339(ms: i64, tz: &Timezone) -> Result<String, String> {
    DateTime::from_unix_ms(ms).format("%Y-%m-%dT%H:%M:%S%.3f%:z", tz)
}

#[cfg(test)]
mod tests;
