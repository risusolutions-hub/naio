//! Month grids and calendar layout (~Python `calendar` module).

use crate::date::Date;
use crate::error::CalError;

/// Flat list of day numbers for a month (includes only real days).
///
/// >>> use niao_cal::month_days;
/// >>> month_days(2026, 2)
/// [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28]
pub fn month_days(year: i32, month: u32) -> Result<Vec<u32>, CalError> {
    let d = Date::new(year, month, 1)?;
    let last = Date::new(year, month, niao_time::days_in_month(year, month))?;
    let mut out = Vec::with_capacity((last.day()) as usize);
    let mut cur = d;
    loop {
        out.push(cur.day());
        if cur.to_days() >= last.to_days() {
            break;
        }
        cur = cur.add_days(1);
    }
    Ok(out)
}

/// Month matrix for display: rows of weeks, `0` pads empty cells.
///
/// `first_weekday` is the column index of the first day of the week (Mon=0).
///
/// >>> use niao_cal::month_matrix;
/// >>> let m = month_matrix(2026, 7, 0).unwrap();
/// >>> m[0].len()
/// 7
pub fn month_matrix(year: i32, month: u32, first_weekday: u8) -> Result<Vec<Vec<u32>>, CalError> {
    if first_weekday > 6 {
        return Err(CalError::InvalidWeekday(first_weekday as u32));
    }
    let first = Date::new(year, month, 1)?;
    let dim = niao_time::days_in_month(year, month);
    let start_col = (first.weekday() + 7 - first_weekday) % 7;
    let mut weeks: Vec<Vec<u32>> = Vec::new();
    let mut week = vec![0u32; 7];
    let mut col = start_col as usize;
    for day in 1..=dim {
        week[col] = day;
        col += 1;
        if col == 7 {
            weeks.push(week);
            week = vec![0u32; 7];
            col = 0;
        }
    }
    if col > 0 || weeks.is_empty() {
        weeks.push(week);
    }
    Ok(weeks)
}

/// Iterator-friendly month dates.
pub fn iter_month(year: i32, month: u32) -> Result<Vec<Date>, CalError> {
    let start = Date::new(year, month, 1)?;
    let end = Date::new(year, month, niao_time::days_in_month(year, month))?;
    Ok(crate::date::date_range(start, end))
}

/// Number of weeks spanned by a month grid.
pub fn month_weeks(year: i32, month: u32, first_weekday: u8) -> Result<u32, CalError> {
    Ok(month_matrix(year, month, first_weekday)?.len() as u32)
}

/// Week-of-month (1-based) for a date, relative to `first_weekday`.
pub fn week_of_month(date: Date, first_weekday: u8) -> Result<u32, CalError> {
    if first_weekday > 6 {
        return Err(CalError::InvalidWeekday(first_weekday as u32));
    }
    let first = Date::new(date.year(), date.month(), 1)?;
    let offset = (first.weekday() + 7 - first_weekday) % 7;
    let dom = date.day();
    Ok((dom + u32::from(offset) - 1) / 7 + 1)
}

/// Nth weekday of month (e.g. 3rd Monday). `nth` is 1-based; negative counts from end.
///
/// >>> use niao_cal::nth_weekday_of_month;
/// >>> let d = nth_weekday_of_month(2026, 1, 0, 3).unwrap();
/// >>> assert_eq!(d.format_iso(), "2026-01-19");
pub fn nth_weekday_of_month(
    year: i32,
    month: u32,
    weekday: u8,
    nth: i32,
) -> Result<Date, CalError> {
    if weekday > 6 {
        return Err(CalError::InvalidWeekday(weekday as u32));
    }
    if nth == 0 {
        return Err(CalError::RangeError("nth must be non-zero".into()));
    }
    let dim = niao_time::days_in_month(year, month);
    if nth > 0 {
        let first = Date::new(year, month, 1)?;
        let first_wd = first.weekday();
        let delta = ((weekday as i32 + 7 - first_wd as i32) % 7) as i32 + (nth - 1) * 7;
        let day = 1 + delta;
        if day > dim as i32 {
            return Err(CalError::RangeError(format!(
                "no {nth}th weekday {weekday} in {year}-{month:02}"
            )));
        }
        return Date::new(year, month, day as u32);
    }
    let last = Date::new(year, month, dim)?;
    let last_wd = last.weekday();
    let delta = ((last_wd as i32 + 7 - weekday as i32) % 7) as i32 + (-nth - 1) * 7;
    let day = dim as i32 - delta;
    if day < 1 {
        return Err(CalError::RangeError(format!(
            "no {nth}th weekday {weekday} from end in {year}-{month:02}"
        )));
    }
    Date::new(year, month, day as u32)
}

/// Observe a fixed holiday on nearest weekday when it falls on weekend.
pub fn observe_weekend(date: Date, weekend: &[bool; 7]) -> Date {
    if !weekend[date.weekday() as usize] {
        return date;
    }
    if date.weekday() == 5 {
        // Saturday -> Friday
        date.add_days(-1)
    } else {
        // Sunday -> Monday
        date.add_days(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn third_monday_jan_2026() {
        let d = nth_weekday_of_month(2026, 1, 0, 3).unwrap();
        assert_eq!(d.format_iso(), "2026-01-19");
    }

    #[test]
    fn matrix_july_2026() {
        let m = month_matrix(2026, 7, 0).unwrap();
        assert_eq!(m.len(), 5);
        assert_eq!(m[0][2], 1);
    }
}
