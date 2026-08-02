# ncal — calendar math

Business days, holiday tables, ISO week numbers, and month grids. ~Python `calendar` + `workalendar` subset.

## Import

```niao
import "ncal"
```

Paths `import "std/ncal"` and `import "ncal"` are equivalent. Flat builtins (`ncal_parse`, `ncal_add_business_days`, …) are also available globally after import.

## Quick start

```niao
import "ncal"

// Parse and format civil dates
let d = ncal.parse("2026-07-13")
print(ncal.format(d))                    // 2026-07-13
print(ncal.weekday(d), ncal.iso_week(d)) // Mon=0, ISO week object

// Business days (Sat/Sun weekend by default)
let fri = ncal.date(2026, 7, 10)
let mon = ncal.add_business_days(fri, 1)
print(mon.iso)                           // 2026-07-13

// Month grid (0 = padding cell)
let grid = ncal.month_matrix(2026, 7)
print(len(grid), len(grid[0]))           // 5 rows, 7 columns

// Holiday calendar with US federal preset
let cal = ncal.calendar({preset: "us_federal", year: 2026})
print(ncal.cal_is_working(cal, ncal.date(2026, 7, 4)))  // false (Independence Day)
print(ncal.cal_working_between(cal, ncal.date(2026, 7, 1), ncal.date(2026, 7, 31)))
ncal.cal_close(cal)
```

## Date objects

Parsed and constructed dates are objects:

| Field | Description |
|-------|-------------|
| `year`, `month`, `day` | Civil components |
| `weekday` | Monday = 0 … Sunday = 6 |
| `ordinal` | Day of year (1-based) |
| `quarter` | Calendar quarter 1..4 |
| `iso_year`, `iso_week` | ISO 8601 week-year and week number |
| `iso` | `YYYY-MM-DD` string shorthand |

Dates also accept ISO strings anywhere a date is expected.

## Civil calendar

| Method | Description |
|--------|-------------|
| `ncal.date(year, month, day)` | Construct a validated date (error object on invalid). |
| `ncal.parse(text)` | Parse `YYYY-MM-DD` or `YYYYMMDD`. |
| `ncal.format(date, fmt?)` | Format with strftime-like pattern (default `%Y-%m-%d`). |
| `ncal.valid(year, month, day)` | `true` when the civil date exists. |
| `ncal.leap_year(year)` | Leap-year test. |
| `ncal.days_in_month(year, month)` | Days in month. |
| `ncal.today(tz?)` | Today's civil date in timezone (default `UTC`). |
| `ncal.weekday(date)` | Weekday index Mon=0. |
| `ncal.iso_week(date)` | `{year, week, weekday}` ISO week. |
| `ncal.ordinal(date)` | Day of year. |
| `ncal.quarter(date)` | Quarter 1..4. |
| `ncal.add_days(date, n)` | Add signed calendar days. |
| `ncal.diff_days(a, b)` | Signed calendar-day difference `b - a`. |
| `ncal.range(start, end)` | Inclusive array of dates. |
| `ncal.weekdays()` | Full weekday names (Mon first). |
| `ncal.months()` | Full month names. |

### Format codes

| Code | Output |
|------|--------|
| `%Y` | 4-digit year |
| `%m` | 2-digit month |
| `%d` | 2-digit day |
| `%j` | 3-digit ordinal |
| `%Q` | Quarter |
| `%W` | 2-digit ISO week |
| `%w` | Weekday number |
| `%a` / `%A` | Abbrev / full weekday name |
| `%b` / `%B` | Abbrev / full month name |

## Month grids

| Method | Description |
|--------|-------------|
| `ncal.month_days(year, month)` | Flat list of day numbers. |
| `ncal.month_matrix(year, month, first_weekday?)` | Week rows; `0` pads leading/trailing cells. |
| `ncal.month_weeks(year, month, first_weekday?)` | Number of week rows. |
| `ncal.iter_month(year, month)` | Array of date objects for the month. |
| `ncal.nth_weekday(year, month, weekday, nth)` | Nth weekday (negative `nth` counts from month end). |
| `ncal.week_of_month(date, first_weekday?)` | 1-based week index within month. |

`first_weekday` is the column for Monday=0 … Sunday=6 (default `0`).

## Business days (no holidays)

Weekend options object (optional last argument):

| Key | Default | Description |
|-----|---------|-------------|
| `weekend` | `[5, 6]` | Weekday indices treated as non-working (Sat, Sun). |
| `workweek` | — | Alternative: list working days, e.g. `[0,1,2,3,4]`. |

| Method | Description |
|--------|-------------|
| `ncal.is_weekend(date, opts?)` | Weekend membership. |
| `ncal.is_weekday(date, opts?)` | Inverse of `is_weekend`. |
| `ncal.add_business_days(date, n, opts?)` | Add signed business days. |
| `ncal.business_days_between(start, end, opts?)` | Inclusive business-day count. |
| `ncal.next_business_day(date, include_self?, opts?)` | Next business day. |
| `ncal.prev_business_day(date, include_self?, opts?)` | Previous business day. |
| `ncal.batch_is_weekday(dates, opts?)` | Parallel weekday flags for an array. |

## Work calendars (holidays)

`ncal.calendar(opts?)` returns an integer handle. Close with `ncal.cal_close(handle)` when done.

### Calendar options

| Key | Description |
|-----|-------------|
| `weekend` | Non-working weekday indices (default Sat/Sun). |
| `workweek` | Working-day indices (alternative to `weekend`). |
| `holidays` | Array of dates to seed the table. |
| `preset` | `"us_federal"` or `"uk_bank"` (uses `year`, default 2026). |
| `year` | Year for preset holiday generation. |

| Method | Description |
|--------|-------------|
| `ncal.us_federal_calendar(year)` | Handle preloaded with US federal holidays. |
| `ncal.cal_add_holiday(cal, date)` | Add one holiday. |
| `ncal.cal_add_holidays(cal, dates)` | Add many; returns count added. |
| `ncal.cal_remove_holiday(cal, date)` | Remove; returns `true` if present. |
| `ncal.cal_clear(cal)` | Remove all holidays. |
| `ncal.cal_is_holiday(cal, date)` | Holiday membership. |
| `ncal.cal_is_working(cal, date)` | Working day (not weekend, not holiday). |
| `ncal.cal_holidays(cal, year)` | Sorted holidays in a year. |
| `ncal.cal_add_working(cal, date, n)` | Add signed working days. |
| `ncal.cal_working_between(cal, start, end)` | Inclusive working-day count. |
| `ncal.cal_next_working(cal, date, include_self?)` | Next working day. |
| `ncal.cal_prev_working(cal, date, include_self?)` | Previous working day. |
| `ncal.cal_batch_working(cal, dates)` | Parallel working-day flags. |
| `ncal.cal_count(cal)` | Number of stored holidays. |
| `ncal.cal_close(cal)` | Release handle. |

## Presets & helpers

| Method | Description |
|--------|-------------|
| `ncal.easter(year)` | Western Easter Sunday. |
| `ncal.us_federal(year)` | US federal holiday dates (observed). |
| `ncal.uk_bank(year)` | UK England & Wales bank holidays. |

## Errors

Invalid dates, bad handles, and parse failures return `ncal_error` objects (or arity/type `RuntimeError`s). Check with `nvalid.is_error(result)` when needed.

## Deferred / out of scope

- Locale-specific holiday names and i18n month labels (use `nunicode` / `nfmt` for display).
- Lunar / Hebrew / Islamic calendars.
- Per-region workalendar parity beyond US federal and UK bank presets (add custom holidays via `cal_add_holiday`).
- Time-of-day or timezone-aware datetime math (use `time` + `ncal` together).

## See also

- [`TIME.md`](TIME.md) — wall clock, zones, parsing datetimes
- [`NICAL.md`](NICAL.md) — iCalendar files and RRULE recurrence
