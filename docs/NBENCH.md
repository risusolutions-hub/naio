# nbench standard library

Micro-benchmark harness: `run(name, fn, opts?)` with warmup, nanosecond samples, and **mean / p50 / p95 / p99** stats. Compare stored runs by name or result object.

## Import

```niao
import "nbench"
```

Paths `import "std/nbench"` and `import "nbench"` are equivalent. Flat builtins (`nbench_run`, `nbench_compare`, …) are also available globally after import.

## Quick start

```niao
import "nbench"

fn work() {
    let s = 0
    for i in 0..1000 { s = s + i }
    return s
}

let r = nbench.run("loop", work, {warmup: 2, iterations: 20})
print(r.name, r.n, r.mean, r.p50, r.p95, r.p99)

let r2 = nbench.run("loop2", work)
let cmp = nbench.compare("loop", "loop2")
print(cmp.faster, cmp.ratio)
```

## Functions

| Method | Description |
|--------|-------------|
| `nbench.run(name, fn, opts?)` | Warm up then time `fn` repeatedly. `opts`: `{warmup: 3, iterations: 10}` (defaults). Returns `{name, n, warmup, mean, min, max, p50, p95, p99}` (times in **nanoseconds**). Stores result by `name`. Catchable error if `fn` fails or returns an error value. |
| `nbench.compare(a, b)` | Compare two runs by name (`string`) or result object. Returns `{a, b, delta_mean, ratio, faster}`. `faster` is the name with lower mean, or `"tie"`. |
| `nbench.get(name)` | Retrieve a stored result object. Catchable error if not found. |
| `nbench.stats(samples)` | Compute stats from an `IntArray` or `Array` of int samples (nanoseconds). |
| `nbench.clear(name?)` | Drop one stored result or all results. Returns `nil`. |

## Notes

- Timing uses monotonic `Instant`; samples are raw elapsed nanoseconds per iteration.
- Percentiles use linear interpolation on sorted samples (same approach as `nprofile`).
- Results are stored in a thread-local map keyed by benchmark name.

## Errors

| Code | Meaning |
|------|---------|
| 3170 | Wrong argument count. |
| 3171 | Benchmark error (fn failure, missing result) — catchable `nbench_error`. |
| 3172 | Wrong argument type. |
