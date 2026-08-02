//! Holiday tables and working-day calendars (~workalendar).

use crate::business::{
    business_days_between_fast, default_weekend, is_weekday, is_weekend, weekend_from_days,
};
use crate::date::Date;
use crate::error::CalError;
use rayon::prelude::*;
use std::collections::BTreeSet;

/// Working-day calendar with configurable weekends and holiday set.
#[derive(Debug, Clone)]
pub struct WorkCalendar {
    weekend: [bool; 7],
    holidays: BTreeSet<i32>,
}

impl WorkCalendar {
    /// Create a calendar; `weekend_days` lists weekday indices (Mon=0) treated as non-working.
    ///
    /// >>> use niao_cal::WorkCalendar;
    /// >>> let cal = WorkCalendar::new(&[5, 6]).unwrap();
    /// >>> cal.is_working_day(Date::new(2026, 7, 13).unwrap())
    /// true
    pub fn new(weekend_days: &[u8]) -> Result<Self, CalError> {
        Ok(Self {
            weekend: if weekend_days.is_empty() {
                default_weekend()
            } else {
                weekend_from_days(weekend_days)?
            },
            holidays: BTreeSet::new(),
        })
    }

    pub fn with_holidays(weekend_days: &[u8], holidays: &[Date]) -> Result<Self, CalError> {
        let mut cal = Self::new(weekend_days)?;
        for h in holidays {
            cal.add_holiday(*h);
        }
        Ok(cal)
    }

    pub fn weekend_mask(&self) -> [bool; 7] {
        self.weekend
    }

    pub fn add_holiday(&mut self, date: Date) {
        self.holidays.insert(date.to_days());
    }

    pub fn remove_holiday(&mut self, date: Date) -> bool {
        self.holidays.remove(&date.to_days())
    }

    pub fn clear_holidays(&mut self) {
        self.holidays.clear();
    }

    pub fn holiday_count(&self) -> usize {
        self.holidays.len()
    }

    pub fn is_holiday(&self, date: Date) -> bool {
        self.holidays.contains(&date.to_days())
    }

    pub fn is_working_day(&self, date: Date) -> bool {
        !is_weekend(date, &self.weekend) && !self.is_holiday(date)
    }

    pub fn holidays_in_year(&self, year: i32) -> Vec<Date> {
        let start = Date::new(year, 1, 1).unwrap().to_days();
        let end = Date::new(year, 12, 31).unwrap().to_days();
        self.holidays
            .range(start..=end)
            .map(|&d| Date::from_days(d))
            .collect()
    }

    pub fn add_working_days(&self, date: Date, n: i32) -> Date {
        if n == 0 {
            return date;
        }
        let step = if n > 0 { 1 } else { -1 };
        let mut remaining = n.abs();
        let mut cur = date;
        while remaining > 0 {
            cur = cur.add_days(step);
            if self.is_working_day(cur) {
                remaining -= 1;
            }
        }
        cur
    }

    /// Inclusive count of working days between two dates.
    pub fn working_days_between(&self, start: Date, end: Date) -> i32 {
        if start.to_days() > end.to_days() {
            return -self.working_days_between(end, start);
        }
        let base = business_days_between_fast(start, end, &self.weekend);
        let holiday_hits = self.count_holidays_on_weekdays(start, end);
        base - holiday_hits
    }

    fn count_holidays_on_weekdays(&self, start: Date, end: Date) -> i32 {
        let a = start.to_days();
        let b = end.to_days();
        self.holidays
            .range(a..=b)
            .filter(|&&d| {
                let date = Date::from_days(d);
                is_weekday(date, &self.weekend)
            })
            .count() as i32
    }

    pub fn next_working_day(&self, date: Date, include_self: bool) -> Date {
        let mut cur = if include_self { date } else { date.add_days(1) };
        while !self.is_working_day(cur) {
            cur = cur.add_days(1);
        }
        cur
    }

    pub fn prev_working_day(&self, date: Date, include_self: bool) -> Date {
        let mut cur = if include_self {
            date
        } else {
            date.add_days(-1)
        };
        while !self.is_working_day(cur) {
            cur = cur.add_days(-1);
        }
        cur
    }

    /// Parallel batch working-day check.
    pub fn batch_is_working(&self, dates: &[Date]) -> Vec<bool> {
        let weekend = self.weekend;
        let holidays = &self.holidays;
        dates
            .par_iter()
            .map(|d| !is_weekend(*d, &weekend) && !holidays.contains(&d.to_days()))
            .collect()
    }

    /// Merge another holiday set into this calendar.
    pub fn merge(&mut self, other: &WorkCalendar) {
        self.holidays.extend(other.holidays.iter().copied());
    }
}

impl Default for WorkCalendar {
    fn default() -> Self {
        Self::new(&[5, 6]).expect("default weekend")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;

    #[test]
    fn holiday_skips() {
        let mut cal = WorkCalendar::new(&[5, 6]).unwrap();
        cal.add_holiday(Date::new(2026, 7, 13).unwrap());
        assert!(!cal.is_working_day(Date::new(2026, 7, 13).unwrap()));
        let next = cal.add_working_days(Date::new(2026, 7, 10).unwrap(), 1);
        assert_eq!(next.format_iso(), "2026-07-14");
    }
}
