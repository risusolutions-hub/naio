# ncrash standard library

Structured JSON crash reports, `wrap(fn)` guard, and stable **fingerprints** for grouping failures.

## Import

```niao
import "ncrash"
```

Paths `import "std/ncrash"` and `import "ncrash"` are equivalent. Flat builtins (`ncrash_report`, `ncrash_wrap`, …) are also available globally after import.

## Quick start

```niao
import "ncrash"

fn risky() {
    return error(500, "boom")
}

let out = ncrash.wrap(risky, {route: "/api"})
if out.kind != nil {
    print(out.fingerprint, out.message)
    print(ncrash.format(out))
}

let manual = ncrash.report(error(404, "missing"), {id: 7})
print(ncrash.fingerprint(manual))
```

## Report object

| Field | Description |
|-------|-------------|
| `fingerprint` | 8-hex FNV-1a fingerprint from `kind|code|message` |
| `kind` | `"error"`, `"runtime"`, or `"crash"` |
| `code` | Error code (`int`) |
| `message` | Human-readable message |
| `ts_ms` | Wall-clock unix milliseconds |
| `line` / `col` | Call-site span |
| `context` | Optional object passed to `report` / `wrap` |

## Functions

| Method | Description |
|--------|-------------|
| `ncrash.report(err, context?)` | Build a report from an error value (or other value). Stores as `last`. |
| `ncrash.wrap(fn, context?)` | Call `fn` with no args. On success returns the fn result. On error value or runtime failure returns a report object instead of propagating. |
| `ncrash.fingerprint(report_or_err)` | Stable 8-hex fingerprint string. |
| `ncrash.format(report_or_err)` | JSON string of the report. |
| `ncrash.last()` | Most recent report object or `nil`. |
| `ncrash.clear()` | Clear `last`. |

## Fingerprints

Fingerprints hash `kind`, `code`, and `message` with FNV-1a 64 (first 8 hex chars). Identical failures produce identical fingerprints for deduplication and crash grouping.

## Errors

| Code | Meaning |
|------|---------|
| 3190 | Wrong argument count. |
| 3191 | Operation error — catchable `ncrash_error`. |
| 3192 | Wrong argument type. |
