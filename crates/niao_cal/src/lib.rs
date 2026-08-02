//! `ncal` — calendar math for Niao (business days, holidays, month grids).

mod business;
mod calendar;
mod date;
mod error;
mod holidays;
mod presets;

pub use business::{
    add_business_days, batch_is_weekday, business_days_between, business_days_between_fast,
    default_weekend, ensure_ordered, is_weekday, is_weekend, next_business_day, prev_business_day,
    weekend_from_days,
};
pub use calendar::{
    iter_month, month_days, month_matrix, month_weeks, nth_weekday_of_month, observe_weekend,
    week_of_month,
};
pub use date::{
    date_range, days_in_month_of, diff_days, format_date, leap_year, month_names, parse_date,
    valid_date, weekday_names, Date,
};
pub use error::CalError;
pub use holidays::WorkCalendar;
pub use presets::{easter_sunday, uk_bank_holidays, us_federal_calendar, us_federal_holidays};
