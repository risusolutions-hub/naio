//! Weekend / business-day arithmetic without a holiday table.

use crate::date::{diff_days, Date};
use crate::error::CalError;
use rayon::prelude::*;

/// Default weekend mask: Saturday and Sunday.
pub fn default_weekend() -> [bool; 7] {
    let mut w = [false; 7];
    w[5] = true;
    w[6] = true;
    w
}

/// Build weekend mask from weekday indices (Mon=0).
pub fn weekend_from_days(days: &[u8]) -> Result<[bool; 7], CalError> {
    let mut w = [false; 7];
    for &d in days {
        if d > 6 {
            return Err(CalError::InvalidWeekday(d as u32));
        }
        w[d as usize] = true;
    }
    Ok(w)
}

/// `true` when the date falls on a configured weekend day.
///
/// >>> use niao_cal::{Date, is_weekend, default_weekend};
/// >>> is_weekend(Date::new(2026, 7, 11).unwrap(), &default_weekend())
/// true
pub fn is_weekend(date: Date, weekend: &[bool; 7]) -> bool {
    weekend[date.weekday() as usize]
}

/// `true` when the date is not a weekend day.
pub fn is_weekday(date: Date, weekend: &[bool; 7]) -> bool {
    !is_weekend(date, weekend)
}

/// Add `n` business days (weekend-skipping). `n` may be negative.
///
/// >>> use niao_cal::{Date, add_business_days, default_weekend};
/// >>> add_business_days(Date::new(2026, 7, 10).unwrap(), 1, &default_weekend()).format_iso()
/// "2026-07-13"
pub fn add_business_days(date: Date, n: i32, weekend: &[bool; 7]) -> Date {
    if n == 0 {
        return date;
    }
    let step = if n > 0 { 1 } else { -1 };
    let mut remaining = n.abs();
    let mut cur = date;
    while remaining > 0 {
        cur = cur.add_days(step);
        if is_weekday(cur, weekend) {
            remaining -= 1;
        }
    }
    cur
}

/// Count business days from `start` through `end` inclusive.
///
/// >>> use niao_cal::{Date, business_days_between, default_weekend};
/// >>> business_days_between(Date::new(2026, 7, 10).unwrap(), Date::new(2026, 7, 13).unwrap(), &default_weekend())
/// 2
pub fn business_days_between(start: Date, end: Date, weekend: &[bool; 7]) -> i32 {
    let (from, to, sign) = if start.to_days() <= end.to_days() {
        (start, end, 1)
    } else {
        (end, start, -1)
    };
    let mut count = 0i32;
    let mut cur = from;
    loop {
        if is_weekday(cur, weekend) {
            count += 1;
        }
        if cur.to_days() >= to.to_days() {
            break;
        }
        cur = cur.add_days(1);
    }
    count * sign
}

/// Next business day strictly after `date` (or same if already business and `include_self`).
pub fn next_business_day(date: Date, weekend: &[bool; 7], include_self: bool) -> Date {
    let mut cur = if include_self { date } else { date.add_days(1) };
    while is_weekend(cur, weekend) {
        cur = cur.add_days(1);
    }
    cur
}

/// Previous business day strictly before `date` (or same if already business and `include_self`).
pub fn prev_business_day(date: Date, weekend: &[bool; 7], include_self: bool) -> Date {
    let mut cur = if include_self {
        date
    } else {
        date.add_days(-1)
    };
    while is_weekend(cur, weekend) {
        cur = cur.add_days(-1);
    }
    cur
}

/// Parallel batch: mark which dates are business days.
pub fn batch_is_weekday(dates: &[Date], weekend: &[bool; 7]) -> Vec<bool> {
    dates.par_iter().map(|d| is_weekday(*d, weekend)).collect()
}

/// Fast path for long spans; delegates to the exact counter (week-chunking TBD).
pub fn business_days_between_fast(start: Date, end: Date, weekend: &[bool; 7]) -> i32 {
    business_days_between(start, end, weekend)
}

/// Signed calendar span check.
pub fn ensure_ordered(start: Date, end: Date) -> Result<(Date, Date), CalError> {
    if diff_days(start, end) < 0 {
        return Err(CalError::RangeError(
            "end date must be on or after start date".into(),
        ));
    }
    Ok((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;

    #[test]
    fn friday_plus_one() {
        let fri = Date::new(2026, 7, 10).unwrap();
        let mon = add_business_days(fri, 1, &default_weekend());
        assert_eq!(mon.format_iso(), "2026-07-13");
    }
}
