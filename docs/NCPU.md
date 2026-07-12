# ncpu standard library

CPU detection, live usage and temperature readings, cooperative core limits, and worker-thread recommendations. `threads()` is the count every Niao worker pool should use — it honors the user limit and the `ndevice` thermal throttle.

## Import

```niao
import "ncpu"
```

Paths `import "std/ncpu"` and `import "ncpu"` are equivalent. Flat builtins (`ncpu_count`, `ncpu_threads`, …) are also available globally after import.

## Quick start

```niao
import "ncpu"

print(ncpu.count())           // logical cores (>= 1)
print(ncpu.arch())            // e.g. "x86_64"
print(ncpu.brand())           // CPU marketing name
print(ncpu.threads())         // workers to use right now

ncpu.set_limit(50)            // cap at 50% of logical cores
print(ncpu.get_limit())       // 50
print(ncpu.threads())         // reduced thread count

let info = ncpu.info()
print(info.cores, info.limit_pct, info.threads)
```

## Functions

| Method | Description |
|--------|-------------|
| `ncpu.count()` | Logical CPU core count (`int`, always ≥ 1). |
| `ncpu.physical_count()` | Physical core count, or `nil` when the platform cannot report it. |
| `ncpu.arch()` | Target architecture string (e.g. `"x86_64"`, `"aarch64"`). |
| `ncpu.brand()` | CPU brand / model string from platform probes. |
| `ncpu.usage()` | System-wide CPU usage percent (`float`), or `nil` when unavailable. |
| `ncpu.temp_c()` | CPU package temperature in °C (`int`), or `nil` when unavailable. |
| `ncpu.set_limit(pct)` | Limit Niao's CPU appetite to `pct` percent of logical cores (`1..=100`). Returns `nil`. |
| `ncpu.get_limit()` | Current limit percent (`int`, default 100). |
| `ncpu.threads()` | Worker count to use now — limit and thermal throttle applied (`int`, ≥ 1). |
| `ncpu.set_max_temp(c)` | Arm overheat protection for the `ndevice` guard (`0..=110`; `0` disables). Returns `nil`. |
| `ncpu.get_max_temp()` | Configured max CPU temperature (`int`). |
| `ncpu.info()` | Snapshot object: `{cores, physical_cores, arch, brand, usage, temp_c, limit_pct, max_temp_c, threads}`. Unavailable readings are `nil`. |

## Limits and threads

`set_limit(40)` means Niao should use at most 40% of logical cores. `threads()` applies that cap and any global throttle from `ndevice.guard_start()`. Call `ncpu.threads()` (or `ndevice.threads()`) when sizing worker pools.

```niao
ncpu.set_limit(75)
ncpu.set_max_temp(90)
let workers = ncpu.threads()
// spawn workers workers ...
ncpu.set_limit(100)
ncpu.set_max_temp(0)
```

## Errors

| Code | Meaning |
|------|---------|
| 2700 | Wrong argument count. |
| 2701 | Operation error — catchable `ncpu_error`. |
| 2702 | Wrong argument type (e.g. limit out of range). |
