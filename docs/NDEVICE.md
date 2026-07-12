# ndevice standard library

The device brain that ties `ncpu`, `ngpu`, `nram`, and `nnpu` together: full hardware detection, power/thermal profiles, a background safety guard, cooperative pacing, worker counts, and device selection for AI tasks.

## Import

```niao
import "ndevice"
```

Paths `import "std/ndevice"` and `import "ndevice"` are equivalent. Flat builtins (`ndevice_detect`, `ndevice_pace`, …) are also available globally after import.

## Quick start

```niao
import "ndevice"

print(ndevice.summary())      // one-line hardware summary
let hw = ndevice.detect()     // {cpu, gpu, ram, npu, os}

ndevice.profile("balanced")   // preset limits + max temps
ndevice.guard_start({interval_ms: 1000})

for step in 0..1000 {
    ndevice.pace()            // auto-sleep when hot
    // ... training / decode work ...
}

print(ndevice.status())       // guard state + throttle level
print(ndevice.best_device("infer"))   // "gpu" | "npu" | "cpu"
ndevice.guard_stop()
```

## Functions

| Method | Description |
|--------|-------------|
| `ndevice.detect()` | Full report: `{cpu, gpu, ram, npu, os}`. Each subsection mirrors the corresponding module's snapshot fields. |
| `ndevice.summary()` | Human-readable one-liner: brand · cores · RAM · GPU · NPU. |
| `ndevice.profile(name)` | Apply preset `"eco"`, `"balanced"`, or `"performance"`. Returns `nil`. Unknown name → type error. |
| `ndevice.get_profile()` | Active profile name: `"eco"`, `"balanced"`, `"performance"`, or `"custom"`. |
| `ndevice.set_limits(opts)` | Bulk limits object (any subset): `{cpu_pct, gpu_pct, npu_pct, ram_mb, ram_pct, gpu_max_temp, cpu_max_temp}`. Sets profile to `"custom"`. Returns `nil`. |
| `ndevice.limits()` | Current limits: `{cpu_pct, gpu_pct, npu_pct, ram_mb, gpu_max_temp, cpu_max_temp}`. |
| `ndevice.guard_start(opts?)` | Start background safety monitor. `opts`: `{interval_ms, gpu_max_temp, cpu_max_temp}` or bare `interval_ms` int (`100..=60000`, default 1000). Returns `true` if started. |
| `ndevice.guard_stop()` | Stop the guard. Returns `nil`. |
| `ndevice.guard_running()` | `true` when the guard thread is active. |
| `ndevice.status()` | `{guard_running, ticks, throttle_level, reason, gpu_temp_c, cpu_temp_c, ram_used_pct, threads}`. |
| `ndevice.throttle_level()` | Global throttle `0` ok · `1` warm · `2` hot · `3` critical. |
| `ndevice.set_throttle(level)` | Manual override `0..=3` (also useful in tests). Returns `nil`. |
| `ndevice.pace()` | Cooperative sleep inside hot loops: 0 / 2 / 8 / 25 ms by throttle level. Returns `nil`. |
| `ndevice.threads()` | Worker count under CPU limit + throttle (`int`). |
| `ndevice.ok()` | `true` when throttle level &lt; 2 (safe for new heavy work). |
| `ndevice.best_device(task?)` | `"gpu"` \| `"npu"` \| `"cpu"`. Task: `"train"`, `"infer"`, `"embed"`, or `"auto"` (default). |

## Profiles

| Profile | CPU limit | GPU limit | NPU limit | GPU max °C | CPU max °C |
|---------|-----------|-----------|-----------|------------|------------|
| `eco` | 50% | 50% | 50% | 75 | 85 |
| `balanced` | 75% | 80% | 80% | 80 | 90 |
| `performance` | 100% | 100% | 100% | 85 | 95 |

## Throttle levels

The background guard (or `set_throttle`) sets a global level consulted by all hardware modules:

| Level | Name | Effect |
|-------|------|--------|
| 0 | ok | Full speed — `pace()` no-ops, `ok()` true. |
| 1 | warm | `pace()` sleeps 2 ms. |
| 2 | hot | `pace()` sleeps 8 ms; `ngpu.ok()` / `nnpu.ok()` / `ndevice.ok()` false; threads halved. |
| 3 | critical | `pace()` sleeps 25 ms; threads → 1. |

```niao
ndevice.guard_start({interval_ms: 1000, gpu_max_temp: 80, cpu_max_temp: 90})

while training {
    ndevice.pace()
    if !ndevice.ok() { break }
    // step ...
}
```

## Device selection

| Task | Priority |
|------|----------|
| `"train"` | GPU → CPU |
| `"infer"` / `"embed"` | NPU → GPU → CPU |
| `"auto"` | GPU → NPU → CPU |

Selection respects availability, throttle level, and configured max temperatures.

## Errors

| Code | Meaning |
|------|---------|
| 2740 | Wrong argument count. |
| 2741 | Operation error — catchable `ndevice_error`. |
| 2742 | Wrong argument type (e.g. unknown profile). |
| 2743 | Throttle-related error — catchable `ndevice_error`. |
