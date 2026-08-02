//! iCalendar / vCard parse + emit and RRULE recurrence for Niao (`nical`).
//!
//! Native RFC 5545 / RFC 6350 parser with line unfolding, component trees,
//! property parameters, and round-trip emit. RRULE expansion uses the
//! [`rrule`] crate engine.

mod builder;
mod component;
mod datetime;
mod emit;
mod error;
mod parse;
mod property;
mod rrule;
mod unfold;

pub use builder::{calendar, contact, CalendarBuilder, ContactBuilder, EventBuilder, TodoBuilder};
pub use component::Component;
pub use datetime::{parse_ical_datetime, unix_ms_to_ical, IcalDateTime};
pub use emit::{emit, emit_all, EmitOptions};
pub use error::{IcalError, MAX_BYTES};
pub use parse::{is_valid, parse, parse_all, parse_calendar, parse_contacts, ParseOptions};
pub use property::{escape_value, unescape_value, Property};
pub use rrule::{
    emit_rrule, parse_rrule, rrule_from_map, rrule_occurrences, rrule_to_map, ByDay, Frequency,
    RRule, Weekday,
};

/// Parse with default options.
pub fn parse_one(input: &str) -> Result<Component, IcalError> {
    parse(input, &ParseOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_roundtrip() {
        let cal = calendar()
            .event(|e| {
                e.summary("Demo")
                    .uid("u1")
                    .dtstart("20260105T090000Z")
                    .dtend("20260105T100000Z")
                    .rrule("FREQ=DAILY;COUNT=3")
            })
            .build();
        let text = emit(&cal, &EmitOptions::default()).unwrap();
        let back = parse_calendar(&text, &ParseOptions::default()).unwrap();
        assert_eq!(back.children.len(), 1);
    }

    #[test]
    fn vcard_roundtrip() {
        let c = contact()
            .full_name("Test User")
            .email("t@example.com")
            .uid("c1")
            .build();
        let text = emit(&c, &EmitOptions::default()).unwrap();
        let all = parse_contacts(&text, &ParseOptions::default()).unwrap();
        assert_eq!(all[0].get("FN").unwrap().value, "Test User");
    }
}
