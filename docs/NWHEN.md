# nwhen standard library

Natural-language and fuzzy date parsing for user-facing input, logs, and ETL. Native implementation — Python `dateparser` / `dateutil` subset (extends the `time` library for civil fields and time zones).

## Import

```niao
import "nwhen"
```

Paths `import "std/nwhen"` and `import "nwhen"` are equivalent. Flat builtins (`nwhen_parse`, `nwhen_search`, …) are also available globally after import.

## Quick start

```niao
import "nwhen"
import "time"

let dt = nwhen.parse("next friday 5pm")
print(dt.year, dt.month, dt.day, dt.hour, dt.minute)

let hits = nwhen.search("remind me tomorrow at 3pm about the release")
print(hits[0].text, hits[0].date.unix_ms)

let rows = ["in 2 weeks", "last monday", "March 15 2024"]
let parsed = nwhen.batch(rows)
```

Parsed values are **datetime objects** compatible with `time.decompose` / `time.format` (`year`, `month`, `day`, `hour`, `minute`, `second`, `unix_ms`, `timezone`, …).

## Parse options

Pass an optional options object as the second argument:

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `base_ms` | int | now | Reference instant for relative phrases (`in 2 days`, `next friday`). |
| `timezone` / `tz` | string | `"UTC"` | IANA zone for civil conversion. |
| `prefer` | string | `"future"` | `"future"`, `"past"`, or `"current"` — bias ambiguous weekday-only dates. |
| `date_order` | string | `"mdy"` | `"mdy"`, `"dmy"`, or `"ymd"` for numeric dates like `03/04/2024`. |
| `fuzzy` | bool | `true` | Ignore extra punctuation/whitespace (not typo correction). |
| `require` | string | `"any"` | `"date"`, `"time"`, `"both"`, or `"any"`. |
| `languages` | array | `["en"]` | Language tags (English fully supported). |

```niao
let o = {
    "base_ms": time.now("America/New_York").unix_ms,
    "timezone": "America/New_York",
    "prefer": "future",
    "date_order": "dmy"
}
print(nwhen.parse("next friday 5pm", o))
```

## Supported phrases (English)

| Category | Examples |
|----------|----------|
| Relative | `in 2 weeks`, `in a day`, `3 days ago`, `2 hours ago` |
| Weekdays | `next friday`, `last monday`, `this tuesday` |
| Keywords | `now`, `today`, `tomorrow`, `yesterday`, `tonight` |
| Month names | `March 15`, `15 March 2024`, `March 15, 2024` |
| Numeric | `03/15/2024`, `2024-03-15`, `15-03-2024` (with `date_order`) |
| Time | `5pm`, `5:30 pm`, `17:30`, `noon`, `midnight` |
| Combined | `tomorrow at 5pm`, `next friday 5pm`, `in 2 weeks at noon` |
| ISO / RFC | `2024-03-15T17:30:00Z`, RFC 2822 mail dates |
| Periods | `next week`, `last month`, `end of month` |

## API reference

| Method | Description |
|--------|-------------|
| `nwhen.parse(text, options?)` | Parse a single phrase; returns datetime object or catchable `error`. |
| `nwhen.valid(text, options?)` | `true` when `text` parses under options. |
| `nwhen.parse_many(text, options?)` | Ranked candidate datetime objects (alternate numeric orders). |
| `nwhen.search(text, options?)` | Find parseable substrings; returns `{text, start, end, date}`. |
| `nwhen.batch(texts, options?, threads?)` | Parallel parse; `threads` `0` = auto. |
| `nwhen.languages()` | Supported language tags. |
| `nwhen.to_unix_ms(date_obj, timezone?)` | Extract or compute unix ms from a datetime object. |

## Errors

| Code | Meaning |
|------|---------|
| 4360 | Wrong argument count. |
| 4361 | Could not parse / empty / ambiguous (catchable). |
| 4362 | Type mismatch in arguments. |
| 4363 | Invalid date/time after parse. |

## Notes

- For strict formatted dates, prefer `time.parse` with an explicit format string.
- Use `nwhen` for human-entered text; use `time` for deterministic formatting and arithmetic.
- High-volume ingestion: `batch` + fixed `base_ms` / `timezone` in options.

## Deferred / not in 0.1.0

- Non-English locale packs (structure via `languages`; only `"en"` lexicon today).
- Holiday calendars and named holidays (`Thanksgiving`, etc.).
- Typo-tolerant fuzzy matching beyond whitespace/punctuation normalization.
- Timezone names embedded in free text (`EST`, `PST`) — set `timezone` in options instead.
