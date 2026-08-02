//! Built-in regional holiday generators.

use crate::calendar::{nth_weekday_of_month, observe_weekend};
use crate::date::Date;
use crate::error::CalError;
use crate::holidays::WorkCalendar;

/// Western (Gregorian) Easter Sunday via Anonymous Gregorian algorithm.
///
/// >>> use niao_cal::easter_sunday;
/// >>> easter_sunday(2026).unwrap().format_iso()
/// "2026-04-05"
pub fn easter_sunday(year: i32) -> Result<Date, CalError> {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = ((h + l - 7 * m + 114) / 31) as u32;
    let day = (((h + l - 7 * m + 114) % 31) + 1) as u32;
    Date::new(year, month, day)
}

/// US federal holidays for `year` with Sat/Sun observation rules.
///
/// >>> use niao_cal::us_federal_holidays;
/// >>> us_federal_holidays(2026).len() >= 10
pub fn us_federal_holidays(year: i32) -> Result<Vec<Date>, CalError> {
    let weekend = crate::business::default_weekend();
    let mut out = Vec::with_capacity(12);

    let fixed = [
        (1, 1),   // New Year's
        (6, 19),  // Juneteenth
        (7, 4),   // Independence
        (11, 11), // Veterans
        (12, 25), // Christmas
    ];
    for (m, d) in fixed {
        out.push(observe_weekend(Date::new(year, m, d)?, &weekend));
    }

    out.push(nth_weekday_of_month(year, 1, 0, 3)?); // MLK
    out.push(nth_weekday_of_month(year, 2, 0, 3)?); // Presidents
    out.push(nth_weekday_of_month(year, 5, 0, -1)?); // Memorial (last Mon)
    out.push(nth_weekday_of_month(year, 9, 0, 1)?); // Labor
    out.push(nth_weekday_of_month(year, 10, 0, 2)?); // Columbus
    out.push(nth_weekday_of_month(year, 11, 4, 4)?); // Thanksgiving (4th Thu)

    out.sort_by_key(|d| d.to_days());
    out.dedup_by_key(|d| d.to_days());
    Ok(out)
}

/// UK England & Wales bank holidays (subset).
pub fn uk_bank_holidays(year: i32) -> Result<Vec<Date>, CalError> {
    let weekend = crate::business::default_weekend();
    let easter = easter_sunday(year)?;
    let good_friday = easter.add_days(-2);
    let easter_mon = easter.add_days(1);
    let mut out = vec![
        observe_weekend(Date::new(year, 1, 1)?, &weekend),
        good_friday,
        easter_mon,
        nth_weekday_of_month(year, 5, 0, 1)?,
        nth_weekday_of_month(year, 8, 0, -1)?,
        observe_weekend(Date::new(year, 12, 25)?, &weekend),
        observe_weekend(Date::new(year, 12, 26)?, &weekend),
    ];
    out.sort_by_key(|d| d.to_days());
    Ok(out)
}

/// Build a work calendar preloaded with US federal holidays.
pub fn us_federal_calendar(year: i32) -> Result<WorkCalendar, CalError> {
    let holidays = us_federal_holidays(year)?;
    WorkCalendar::with_holidays(&[5, 6], &holidays)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easter_2026() {
        assert_eq!(easter_sunday(2026).unwrap().format_iso(), "2026-04-05");
    }

    #[test]
    fn us_count() {
        assert!(us_federal_holidays(2026).unwrap().len() >= 10);
    }
}
