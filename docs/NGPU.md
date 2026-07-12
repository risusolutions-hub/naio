# ngpu standard library

GPU detection and live readings (VRAM, utilization, temperature) with cooperative budgets. `set_limit(40)` caps Niao's GPU appetite at 40%; `set_max_temp(80)` arms overheat protection; `ok()` gates new work; `wait_cool()` pauses batch loops until the GPU cools.

Backends: `nvidia-smi` → `rocm-smi` → detection-only fallback. Readings the system cannot provide are `nil` — never invented.

## Import

```niao
import "ngpu"
```

Paths `import "std/ngpu"` and `import "ngpu"` are equivalent. Flat builtins (`ngpu_count`, `ngpu_ok`, …) are also available globally after import.

## Quick start

```niao
import "ngpu"

print(ngpu.available())       // true when at least one GPU is detected
print(ngpu.count())           // number of GPUs
print(ngpu.backend())         // "nvidia-smi" | "rocm-smi" | "detect-only"

if ngpu.available() {
    let g = ngpu.info()       // first GPU (index 0)
    print(g.name, g.vram_total_mb, g.usage, g.temp_c)
}

ngpu.set_limit(80)
ngpu.set_max_temp(85)
print(ngpu.ok())              // false when hot, over limit, or throttled
```

## Functions

| Method | Description |
|--------|-------------|
| `ngpu.available()` | `true` when at least one GPU is detected. |
| `ngpu.count()` | Number of detected GPUs (`int`). |
| `ngpu.backend()` | Probe backend: `"nvidia-smi"`, `"rocm-smi"`, or `"detect-only"`. |
| `ngpu.list()` | Array of GPU objects (see shape below). |
| `ngpu.info(index?)` | One GPU object; default index `0`. Out-of-range index → catchable `ngpu_error`. |
| `ngpu.usage(index?)` | Utilization percent (`int`), or catchable error when unavailable. |
| `ngpu.temp_c(index?)` | Temperature in °C (`int`), or catchable error when unavailable. |
| `ngpu.vram_total_mb(index?)` | Total VRAM in MB, or catchable error when unavailable. |
| `ngpu.vram_used_mb(index?)` | Used VRAM in MB, or catchable error when unavailable. |
| `ngpu.set_limit(pct)` | Advisory GPU budget (`1..=100`). Returns `nil`. |
| `ngpu.get_limit()` | Current limit percent (`int`). |
| `ngpu.set_max_temp(c)` | Overheat threshold (`0..=110`; `0` disables). Returns `nil`. |
| `ngpu.get_max_temp()` | Configured max GPU temperature (`int`). |
| `ngpu.ok(index?)` | `false` when utilization ≥ limit, temp ≥ max, throttle ≥ 2, or index invalid. |
| `ngpu.wait_cool(target?, timeout_ms?)` | Block (250 ms steps) until GPU temp ≤ `target` (default `max_temp − 10`) or `timeout_ms` (default 30000) elapses. Returns `true` when cool, `false` on timeout or unavailable temps. Requires `set_max_temp` when no `target` is given. |
| `ngpu.refresh()` | Force a fresh probe on the next reading. Returns `nil`. |
| `ngpu.status()` | Full snapshot: `{backend, count, limit_pct, max_temp_c, throttle_level, gpus}`. |

## GPU object shape

Each entry in `list()` / `info()` / `status().gpus` has:

| Field | Type | Description |
|-------|------|-------------|
| `index` | int | GPU index. |
| `name` | string | Device name. |
| `vendor` | string | Vendor string. |
| `vram_total_mb` | int \| nil | Total VRAM. |
| `vram_used_mb` | int \| nil | Used VRAM. |
| `usage` | int \| nil | Utilization percent. |
| `temp_c` | int \| nil | Temperature °C. |

## Cooperative gating

```niao
ngpu.set_limit(60)
ngpu.set_max_temp(80)

while work_remaining {
    if !ngpu.ok() {
        ngpu.wait_cool()
        continue
    }
    // run GPU batch ...
}
```

## Errors

| Code | Meaning |
|------|---------|
| 2710 | Wrong argument count. |
| 2711 | GPU error (e.g. index out of range) — catchable `ngpu_error`. |
| 2712 | Wrong argument type. |
| 2713 | Reading unavailable on this system — catchable `ngpu_error`. |
