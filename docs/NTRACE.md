# ntrace standard library

Distributed tracing spans with W3C **`traceparent`** headers, in-span events, JSON export, and a thread-local handle registry.

## Import

```niao
import "ntrace"
```

Paths `import "std/ntrace"` and `import "ntrace"` are equivalent. Flat builtins (`ntrace_start`, `ntrace_export`, …) are also available globally after import.

## Quick start

```niao
import "ntrace"

let root = ntrace.start("request")
print(ntrace.traceparent())   // 00-<trace_id>-<span_id>-01

let child = ntrace.start("db", root)
ntrace.event("query", {table: "users"})
print(ntrace.end(child))

let done = ntrace.end(root)
print(done.duration_ms)

let trace = ntrace.export()
print(trace.finished.len())
```

## W3C traceparent

`ntrace.traceparent(handle?)` returns:

```
00-<32-hex-trace-id>-<16-hex-span-id>-<flags>
```

- Root spans allocate a new `trace_id`; child spans inherit the parent trace and record `parent_span_id`.
- Default flags: `01` (sampled).

## Functions

| Method | Description |
|--------|-------------|
| `ntrace.start(name, parent?)` | Begin a span; returns handle (`int`). Parent defaults to current span. |
| `ntrace.end(h)` | Finish span; returns `{name, trace_id, span_id, parent_span_id, start_ms, end_ms, duration_ms, traceparent, events}`. Invalid handle → catchable `ntrace_error` (3183). |
| `ntrace.event(name, attrs?)` | Append an event to the **current** span. Requires an active span. |
| `ntrace.traceparent(h?)` | W3C header for handle or current span. |
| `ntrace.current()` | Active span handle or `nil`. |
| `ntrace.export(as_json?)` | `{active: [...], finished: [...]}`. Pass `true` for a JSON string. |
| `ntrace.clear()` | Drop all active and finished spans; clears current. |
| `ntrace.close(h)` | Remove an active span without finishing; returns whether it existed. |

## Export shape

Each span object includes sorted event lists:

```json
{
  "name": "db",
  "trace_id": "...",
  "span_id": "...",
  "parent_span_id": "...",
  "start_ms": 1710000000000,
  "end_ms": 1710000000042,
  "duration_ms": 1.25,
  "traceparent": "00-...-...-01",
  "events": [{"name": "query", "t_ms": 1710000000005, "attrs": {"table": "users"}}]
}
```

## Errors

| Code | Meaning |
|------|---------|
| 3180 | Wrong argument count. |
| 3181 | Operation error (no active span, bad parent) — catchable `ntrace_error`. |
| 3182 | Wrong argument type. |
| 3183 | Invalid or closed span handle — catchable `ntrace_error`. |
