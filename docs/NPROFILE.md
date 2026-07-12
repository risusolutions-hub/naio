# nprofile standard library

Micro timing spans, named sample recording, and simple latency stats (`mean` / `min` / `max` / `p50` / `p95`). Spans use monotonic `Instant` handles; wall time is available via `now_ms`.

## Import

```niao
import "nprofile"
```

Paths `import "std/nprofile"` and `import "nprofile"` are equivalent. Flat builtins (`nprofile_start`, `nprofile_stats`, …) are also available globally after import.

## Quick start

```niao
import "nprofile"

let h = nprofile.start("parse")
// ... work ...
let r = nprofile.end(h)
print(r.label, r.ms)   // "parse"  and elapsed milliseconds (float)

nprofile.record("parse", 12)
nprofile.record("parse", 18)
print(nprofile.samples("parse"))   // [12, 18]

let s = nprofile.stats([10, 20, 30, 40, 50])
print(s.n, s.mean, s.p50, s.p95)
```

## Functions

| Method | Description |
|--------|-------------|
| `nprofile.now_ms()` | Current wall-clock unix time in milliseconds (`int`). |
| `nprofile.start(label)` | Begin a timed span. Returns a positive integer handle storing the start `Instant`. |
| `nprofile.span(label)` | Alias of `start` — same handle semantics. |
| `nprofile.end(h)` | Stop span `h`, remove it, and return `{label, ms}`. `ms` is elapsed milliseconds as a float. Invalid/closed handle → catchable `nprofile_error`. |
| `nprofile.stats(ms_array)` | Aggregate `{n, mean, min, max, p50, p95}` from an `IntArray` or `Array` of ints. Empty input → zeros. `mean` is float; others are ints. Percentiles use linear interpolation on the sorted samples. |
| `nprofile.record(label, ms)` | Append `ms` (int or float, rounded) to the thread-local sample list named `label`. Returns `nil`. |
| `nprofile.samples(label)` | Return a copy of recorded samples for `label` as an array of ints (empty if none). |
| `nprofile.clear(label?)` | With a label, drop that sample list. With no args, clear all named samples. Returns `nil`. |

## Spans

Handles are optional opaque ints allocated from a thread-local table. Nest freely by starting multiple spans before ending them (order does not have to be LIFO). Always call `end` once per handle; a second `end` on the same id returns a catchable error.

```niao
let outer = nprofile.span("request")
let inner = nprofile.span("db")
// ...
print(nprofile.end(inner))
print(nprofile.end(outer))
```

## Samples

`record` / `samples` / `clear` share a thread-local map of label → int list. Use them to accumulate timings across loops, then feed into `stats`:

```niao
for i in 0..100 {
    let h = nprofile.start("loop")
    // ...
    let r = nprofile.end(h)
    nprofile.record("loop", r.ms)
}
let s = nprofile.stats(nprofile.samples("loop"))
nprofile.clear("loop")
```

## Errors

| Code | Meaning |
|------|---------|
| 3150 | Wrong argument count. |
| 3151 | Operation error (e.g. invalid/closed span handle) — catchable `nprofile_error`. |
| 3152 | Wrong argument type. |
