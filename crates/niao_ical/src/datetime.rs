use crate::error::IcalError;
use std::fmt;

/// Parsed iCalendar date/time value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IcalDateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub utc: bool,
    pub date_only: bool,
}

impl IcalDateTime {
    /// >>> use niao_ical::datetime::parse_ical_datetime;
    /// >>> let dt = parse_ical_datetime("20260105T090000Z", true).unwrap();
    /// >>> dt.year == 2026 && dt.utc && !dt.date_only
    /// true
    pub fn to_unix_ms(&self) -> Result<i64, IcalError> {
        if self.date_only {
            return Err(IcalError::InvalidDateTime(
                "date-only value has no time component".into(),
            ));
        }
        civil_to_unix_ms(
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            self.utc,
        )
    }

    /// Format as iCalendar DATE-TIME (UTC with Z suffix).
    pub fn format_utc(&self) -> String {
        if self.date_only {
            return format!("{:04}{:02}{:02}", self.year, self.month, self.day);
        }
        format!(
            "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

impl fmt::Display for IcalDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.date_only {
            write!(f, "{:04}{:02}{:02}", self.year, self.month, self.day)
        } else if self.utc {
            write!(
                f,
                "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
                self.year, self.month, self.day, self.hour, self.minute, self.second
            )
        } else {
            write!(
                f,
                "{:04}{:02}{:02}T{:02}{:02}{:02}",
                self.year, self.month, self.day, self.hour, self.minute, self.second
            )
        }
    }
}

/// Parse `VALUE` for DATE or DATE-TIME (optional trailing `Z`).
///
/// >>> use niao_ical::datetime::parse_ical_datetime;
/// >>> parse_ical_datetime("20260105", true).unwrap().date_only
/// true
pub fn parse_ical_datetime(raw: &str, date_only_hint: bool) -> Result<IcalDateTime, IcalError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(IcalError::InvalidDateTime("empty".into()));
    }
    let utc = s.ends_with('Z');
    let core = if utc { &s[..s.len() - 1] } else { s };

    if core.len() == 8 && core.chars().all(|c| c.is_ascii_digit()) {
        let year = core[0..4].parse().map_err(|_| invalid(s))?;
        let month = core[4..6].parse().map_err(|_| invalid(s))?;
        let day = core[6..8].parse().map_err(|_| invalid(s))?;
        return Ok(IcalDateTime {
            year,
            month,
            day,
            hour: 0,
            minute: 0,
            second: 0,
            utc: false,
            date_only: true,
        });
    }

    if date_only_hint && core.len() == 8 {
        return parse_ical_datetime(core, true);
    }

    if core.len() != 15 || core.as_bytes().get(8) != Some(&b'T') {
        return Err(invalid(s));
    }
    let year: i32 = core[0..4].parse().map_err(|_| invalid(s))?;
    let month: u8 = core[4..6].parse().map_err(|_| invalid(s))?;
    let day: u8 = core[6..8].parse().map_err(|_| invalid(s))?;
    let hour: u8 = core[9..11].parse().map_err(|_| invalid(s))?;
    let minute: u8 = core[11..13].parse().map_err(|_| invalid(s))?;
    let second: u8 = core[13..15].parse().map_err(|_| invalid(s))?;
    Ok(IcalDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        utc,
        date_only: false,
    })
}

fn invalid(s: &str) -> IcalError {
    IcalError::InvalidDateTime(s.to_string())
}

fn civil_to_unix_ms(
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    utc: bool,
) -> Result<i64, IcalError> {
    if month < 1 || month > 12 || day < 1 || day > 31 {
        return Err(IcalError::InvalidDateTime("out of range".into()));
    }
    let days = days_from_civil(year, month, day);
    let secs = days as i64 * 86_400 + hour as i64 * 3_600 + minute as i64 * 60 + second as i64;
    if utc {
        Ok(secs * 1_000)
    } else {
        // Treat floating local as UTC for expansion anchor (caller may apply TZID).
        Ok(secs * 1_000)
    }
}

fn days_from_civil(y: i32, m: u8, d: u8) -> i32 {
    let y = y as i64;
    let m = m as i64;
    let d = d as i64;
    let y = if m <= 2 { y - 1 } else { y };
    let era = y / 400;
    let yoe = (y - era * 400) as i32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as i32;
    (era * 146097 + doe as i64 - 719468) as i32
}

/// Convert unix milliseconds to UTC iCal DATE-TIME string.
pub fn unix_ms_to_ical(ms: i64) -> String {
    let secs = ms.div_euclid(1_000);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days as i32);
    let hour = (tod / 3_600) as u8;
    let minute = ((tod % 3_600) / 60) as u8;
    let second = (tod % 60) as u8;
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        y, m, d, hour, minute, second
    )
}

fn civil_from_days(z: i32) -> (i32, u8, u8) {
    let z = z as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u8;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ms() {
        let raw = "20260105T090000Z";
        let dt = parse_ical_datetime(raw, false).unwrap();
        let ms = dt.to_unix_ms().unwrap();
        assert_eq!(unix_ms_to_ical(ms), raw);
    }
}
