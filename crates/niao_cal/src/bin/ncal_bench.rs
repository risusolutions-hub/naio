//! Micro-benchmarks for `niao_cal` hot paths.
//! Run: cargo run -p niao_cal --bin ncal_bench --release

use niao_cal::{
    add_business_days, batch_is_weekday, business_days_between_fast, date_range, default_weekend,
    format_date, month_matrix, parse_date, us_federal_holidays, Date, WorkCalendar,
};
use std::time::Instant;

fn bench<F: FnMut() -> u64>(name: &str, mut f: F, iters: u64) {
    for _ in 0..3 {
        f();
    }
    let start = Instant::now();
    let n = f();
    let elapsed = start.elapsed();
    let mean_ns = elapsed.as_nanos() as f64 / iters as f64;
    println!(
        "{name}: n={n} mean={mean_ns:.0} ns total={} ms",
        elapsed.as_millis()
    );
}

fn main() {
    let weekend = default_weekend();
    let d = Date::new(2026, 7, 13).unwrap();

    bench(
        "parse_date x100k",
        || {
            for _ in 0..100_000 {
                let _ = parse_date("2026-07-13").unwrap();
            }
            100_000
        },
        100_000,
    );

    bench(
        "format_date x100k",
        || {
            for _ in 0..100_000 {
                let _ = format_date(&d, "%Y-%m-%d");
            }
            100_000
        },
        100_000,
    );

    bench(
        "month_matrix x50k",
        || {
            for _ in 0..50_000 {
                let _ = month_matrix(2026, 7, 0).unwrap();
            }
            50_000
        },
        50_000,
    );

    bench(
        "add_business_days x50k",
        || {
            let start = Date::new(2026, 1, 1).unwrap();
            for i in 0..50_000 {
                let _ = add_business_days(start, (i % 40) - 20, &weekend);
            }
            50_000
        },
        50_000,
    );

    let s = Date::new(2020, 1, 1).unwrap();
    let e = Date::new(2030, 12, 31).unwrap();
    bench(
        "business_days_between_fast x100k",
        || {
            for _ in 0..100_000 {
                let _ = business_days_between_fast(s, e, &weekend);
            }
            100_000
        },
        100_000,
    );

    let mut cal = WorkCalendar::new(&[5, 6]).unwrap();
    for h in us_federal_holidays(2026).unwrap() {
        cal.add_holiday(h);
    }
    bench(
        "is_working_day x500k",
        || {
            let mut cur = Date::new(2026, 1, 1).unwrap();
            for _ in 0..500_000 {
                let _ = cal.is_working_day(cur);
                cur = cur.add_days(1);
            }
            500_000
        },
        500_000,
    );

    let span = date_range(
        Date::new(2026, 1, 1).unwrap(),
        Date::new(2026, 12, 31).unwrap(),
    );
    bench(
        "batch_is_weekday x10k",
        || {
            for _ in 0..10_000 {
                let _ = batch_is_weekday(&span, &weekend);
            }
            10_000
        },
        10_000,
    );

    bench(
        "cal.working_days_between x50k",
        || {
            let a = Date::new(2026, 1, 1).unwrap();
            let b = Date::new(2026, 12, 31).unwrap();
            for _ in 0..50_000 {
                let _ = cal.working_days_between(a, b);
            }
            50_000
        },
        50_000,
    );
}
