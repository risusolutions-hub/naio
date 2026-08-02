# nical — iCalendar / vCard parse & generate

iCalendar (RFC 5545) and vCard (RFC 6350) parse + emit with RRULE recurrence expansion. ~icalendar / vobject subset.

## Import

```niao
import "nical"
```

Paths `import "std/nical"` and `import "nical"` are equivalent. Flat builtins (`nical_parse`, `nical_emit`, …) are also available globally after import.

## Quick start

```niao
import "nical"

// Parse a calendar file
let cal = nical.parse_calendar_file("meetings.ics")
for ev in nical.events(cal) {
    print(nical.get(ev, "summary"), nical.get(ev, "dtstart"))
}

// Build and emit
let cal2 = nical.build_calendar({
    events: [{
        summary: "Weekly standup",
        uid: "standup-1",
        dtstart: "20260105T090000Z",
        dtend: "20260105T093000Z",
        rrule: "FREQ=WEEKLY;BYDAY=MO,WE,FR;COUNT=52"
    }]
})
let ics = nical.emit(cal2)

// vCard contacts
let contacts = nical.parse_contacts(vcf_text)
let card = nical.build_contact({full_name: "Ada Lovelace", email: "ada@example.com"})
let vcf = nical.emit(card)

// RRULE expansion
let dates = nical.rrule_between(
    "FREQ=WEEKLY;BYDAY=MO;COUNT=10",
    "20260105T090000Z",
    nil,
    nil,
    10
)
```

## Parse & emit

| Method | Description |
|--------|-------------|
| `nical.parse(text, opts?)` | Parse the first top-level component (auto-detect calendar/contact). |
| `nical.parse_all(text, opts?)` | Parse every root component; returns an array. |
| `nical.parse_calendar(text, opts?)` | Parse `.ics` text; root must be `VCALENDAR`. |
| `nical.parse_contacts(text, opts?)` | Parse `.vcf` text; returns an array of `VCARD` objects. |
| `nical.parse_file(path, opts?)` | Read a file and parse the first component. |
| `nical.valid(text)` | `true` when text is syntactically valid iCal/vCard. |
| `nical.emit(component, opts?)` | Serialize a component object to text. |
| `nical.emit_all(components, opts?)` | Serialize an array of root components (multi-vCard). |
| `nical.emit_file(path, component, opts?)` | Write serialized text to a file; returns `true`. |

### Parse options

| Key | Default | Description |
|-----|---------|-------------|
| `relaxed` | `false` | Tolerate property blocks without `BEGIN:` wrapper. |

### Emit options

| Key | Default | Description |
|-----|---------|-------------|
| `fold_lines` | `true` | Fold lines at 75 octets per RFC 5545. |
| `crlf` | `true` | Use CRLF line endings. |

## Component objects

Parsed and built values are objects with:

| Field | Description |
|-------|-------------|
| `kind` | `"calendar"`, `"contact"`, `"event"`, `"todo"`, `"alarm"`, or component name |
| `name` | Component name (`VCALENDAR`, `VEVENT`, `VCARD`, …) |
| `props` | Map of property name → string value (first occurrence) |
| `properties` | Array of `{name, value, params}` for full fidelity |
| `children` | Nested child components |
| `events` | `VEVENT` children (calendars only) |
| `todos` | `VTODO` children (calendars only) |

Access helpers:

| Method | Description |
|--------|-------------|
| `nical.get(component, name)` | Property value by name (case-insensitive), or `nil`. |
| `nical.events(calendar)` | Array of event objects. |
| `nical.todos(calendar)` | Array of todo objects. |

## Builders

| Method | Description |
|--------|-------------|
| `nical.build_calendar(opts?)` | Build a `VCALENDAR` from `{prodid?, method?, events: [...]}`. |
| `nical.build_contact(opts?)` | Build a `VCARD` from `{full_name?, family?, given?, email?, tel?, org?, uid?}`. |

Event entries in `build_calendar` accept: `summary`, `uid`, `dtstart`, `dtend`, `location`, `description`, `rrule`.

## RRULE recurrence

| Method | Description |
|--------|-------------|
| `nical.parse_rrule(text)` | Parse `FREQ=…` rule into `{freq, interval, count, until, byday, …}`. |
| `nical.emit_rrule(rule)` | Serialize rule object or string to `FREQ=…` form. |
| `nical.rrule_between(rule, dtstart, after_ms?, before_ms?, max_count?)` | Expand occurrences; returns UTC `unix_ms` integers. |

`rule` may be an RRULE string or a parsed rule object from `parse_rrule`.

Supported frequencies: `SECONDLY`, `MINUTELY`, `HOURLY`, `DAILY`, `WEEKLY`, `MONTHLY`, `YEARLY` with `INTERVAL`, `COUNT`, `UNTIL`, `BYDAY`, `BYMONTHDAY`, `BYMONTH`, `WKST`.

**Deferred:** `EXRULE` / `RDATE` / `EXDATE` sets, full `VTIMEZONE` resolution, and JSCalendar/JSContact are not implemented in v0.1.0.

## Date/time helpers

| Method | Description |
|--------|-------------|
| `nical.parse_datetime(text, opts?)` | Parse iCal `DATE` or `DATE-TIME`; returns `{year, month, day, hour, minute, second, utc, date_only, unix_ms?}`. |
| `nical.format_datetime(unix_ms)` | Format UTC milliseconds as `YYYYMMDDTHHMMSSZ`. |

## Size limits

Inputs and outputs are capped at **64 MiB** per operation.

## Errors

| Code | Meaning |
|------|---------|
| 4330 | Wrong argument count. |
| 4331 | I/O or emit failure (catchable `nical_error`). |
| 4332 | Wrong argument type (hard error). |
| 4333 | Parse / invalid property / RRULE error (catchable `nical_error`). |

## Compatibility notes

- Unknown `X-*` properties are preserved through parse → emit round-trips.
- Line unfolding and property parameter parsing follow RFC 5545 / RFC 6350.
- RRULE expansion uses the Rust [`rrule`](https://crates.io/crates/rrule) engine (RFC 5545 recurrence).
