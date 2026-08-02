//! Micro-benchmarks for `niao_ical` hot paths.
//! Run: cargo run -p niao_ical --bin nical_bench --release

use niao_ical::{
    calendar, contact, emit, emit_rrule, parse_calendar, parse_contacts, parse_rrule,
    rrule_occurrences, EmitOptions, ParseOptions,
};
use std::time::Instant;

const SAMPLE_CAL: &str = "\
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Example//EN
BEGIN:VEVENT
UID:bench-1
DTSTAMP:20260105T120000Z
DTSTART:20260105T090000Z
DTEND:20260105T093000Z
SUMMARY:Daily standup with a moderately long title for folding tests
DESCRIPTION:Discuss blockers and plans
LOCATION:Room 42
RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR;COUNT=52
END:VEVENT
BEGIN:VEVENT
UID:bench-2
DTSTART:20260106T140000Z
DTEND:20260106T150000Z
SUMMARY:Review
END:VEVENT
END:VCALENDAR
";

const SAMPLE_VCARD: &str = "\
BEGIN:VCARD
VERSION:4.0
FN:Ada Lovelace
N:Lovelace;Ada;;;
EMAIL:ada@example.com
TEL;TYPE=CELL:+1-555-0100
ORG:Analytical Engines Inc.
END:VCARD
BEGIN:VCARD
VERSION:4.0
FN:Grace Hopper
EMAIL:grace@example.com
END:VCARD
";

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
    bench(
        "parse_calendar x10k",
        || {
            for _ in 0..10_000 {
                let _ = parse_calendar(SAMPLE_CAL, &ParseOptions::default()).unwrap();
            }
            10_000
        },
        10_000,
    );

    bench(
        "parse_contacts x10k",
        || {
            for _ in 0..10_000 {
                let _ = parse_contacts(SAMPLE_VCARD, &ParseOptions::default()).unwrap();
            }
            10_000
        },
        10_000,
    );

    let cal = parse_calendar(SAMPLE_CAL, &ParseOptions::default()).unwrap();
    bench(
        "emit_calendar x10k",
        || {
            for _ in 0..10_000 {
                let _ = emit(&cal, &EmitOptions::default()).unwrap();
            }
            10_000
        },
        10_000,
    );

    bench(
        "parse_rrule x100k",
        || {
            for _ in 0..100_000 {
                let _ = parse_rrule("FREQ=WEEKLY;BYDAY=MO,WE,FR;INTERVAL=2;COUNT=100").unwrap();
            }
            100_000
        },
        100_000,
    );

    let rule = parse_rrule("FREQ=WEEKLY;BYDAY=MO;COUNT=26").unwrap();
    bench(
        "emit_rrule x100k",
        || {
            for _ in 0..100_000 {
                let _ = emit_rrule(&rule);
            }
            100_000
        },
        100_000,
    );

    bench(
        "rrule_occurrences weekly x1k",
        || {
            for _ in 0..1_000 {
                let _ = rrule_occurrences(&rule, "20260105T090000Z", None, None, Some(26)).unwrap();
            }
            1_000
        },
        1_000,
    );

    bench(
        "build_calendar x10k",
        || {
            for i in 0..10_000 {
                let _ = calendar()
                    .event(|e| {
                        e.summary(format!("Event {i}"))
                            .uid(format!("id-{i}"))
                            .dtstart("20260105T090000Z")
                            .dtend("20260105T100000Z")
                    })
                    .build();
            }
            10_000
        },
        10_000,
    );

    bench(
        "build_contact x10k",
        || {
            for i in 0..10_000 {
                let _ = contact()
                    .full_name(format!("User {i}"))
                    .email(format!("u{i}@example.com"))
                    .build();
            }
            10_000
        },
        10_000,
    );
}
