# ncron standard library

Standard 5-field cron expressions (minute, hour, day-of-month, month, day-of-week). Hand-rolled parser — pure functions only, no background scheduler thread.

## Import

```niao
import "ncron"
```

Paths `import "std/ncron"` and `import "ncron"` are equivalent. Flat builtins (`ncron_valid`, `ncron_next`, …) are also available globally after import.

## Quick start

```niao
import "ncron"

print(ncron.valid("*/5 * * * *"))              // true
print(ncron.valid("not cron"))                 // false

let f = ncron.fields("30 2 1 * 1-5")
print(f.minute, f.hour, f.day, f.month, f.weekday)

let next = ncron.next("0 9 * * 1-5")         // next weekday 09:00 (local)
print(next)

let ms = time.parse("2026-07-12 09:00:00", "%Y-%m-%d %H:%M:%S")
print(ncron.match("0 9 * * *", ms))          // true when fields align (local)
```

## Field layout

```
┌───────────── minute (0–59)
│ ┌─────────── hour (0–23)
│ │ ┌───────── day of month (1–31)
│ │ │ ┌─────── month (1–12)
│ │ │ │ ┌───── day of week (0–7; 0 and 7 = Sunday)
│ │ │ │ │
* * * * *
```

## Syntax

| Form | Meaning |
|------|---------|
| `*` | Every value in the field |
| `5` | Exactly 5 |
| `1-5` | Range inclusive |
| `*/5` | Every 5th value from the minimum |
| `10-30/5` | Range with step |
| `1,5,9` | List (OR within the field) |

When **both** day-of-month and day-of-week are restricted (neither is `*`), a timestamp matches if **either** field matches (standard cron OR rule).

All scheduling uses the **local** timezone (same default as `time`).

## Functions

| Method | Description |
|--------|-------------|
| `ncron.valid(expr)` | `true` when `expr` is a valid 5-field cron string. |
| `ncron.next(expr, from_unix_ms?)` | Unix milliseconds of the next matching instant at or after `from_unix_ms` (defaults to now). Catchable `error` on parse failure or if no match is found within one year. |
| `ncron.fields(expr)` | `{minute, hour, day, month, weekday}` with the raw field strings. Catchable `error` on invalid input. |
| `ncron.match(expr, unix_ms)` | `true` when `unix_ms` matches `expr` in local time. Catchable `error` on invalid expression. |

## Errors

| Code | Meaning |
|------|---------|
| 2910 | Wrong argument count. |
| 2911 | Semantic error — e.g. no occurrence within search window (catchable). |
| 2912 | Parse error — invalid cron expression (catchable). |
