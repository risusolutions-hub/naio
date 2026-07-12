# nram standard library

System and process memory readings plus a cooperative RAM budget. Set `set_limit_mb(2048)` or `set_limit_pct(50)`, then gate big allocations with `ok(extra_mb)` and watch `pressure()`.

## Import

```niao
import "nram"
```

Paths `import "std/nram"` and `import "nram"` are equivalent. Flat builtins (`nram_total_mb`, `nram_ok`, …) are also available globally after import.

## Quick start

```niao
import "nram"

print(nram.total_mb())        // system RAM total, or nil
print(nram.process_mb())      // this process RSS in MB
print(nram.pressure())        // "low" | "medium" | "high" | "critical" | "unknown"

nram.set_limit_mb(4096)
print(nram.get_limit_mb())    // 4096
print(nram.ok(512))           // false if 512 MB would exceed budget
print(nram.headroom_mb())     // min(system available, budget remaining)
```

## Functions

| Method | Description |
|--------|-------------|
| `nram.total_mb()` | Total system RAM in MB (`int`), or `nil` when unavailable. |
| `nram.available_mb()` | Available system RAM in MB, or `nil`. |
| `nram.used_mb()` | Used system RAM in MB (`total − available`), or `nil`. |
| `nram.usage()` | System memory usage percent (`float`), or `nil`. |
| `nram.process_mb()` | This process's resident memory in MB (`int`). |
| `nram.set_limit_mb(mb)` | RAM budget in MB (`>= 0`; `0` disables). Returns `nil`. |
| `nram.set_limit_pct(pct)` | RAM budget as percent of total (`0..=100`; `0` disables). Returns `nil`. |
| `nram.get_limit_mb()` | Effective budget in MB after combining mb/pct limits (`0` = unlimited). |
| `nram.ok(extra_mb?)` | `true` when `extra_mb` more memory (default `0`) fits inside budget and system headroom. |
| `nram.pressure()` | `"low"` \| `"medium"` \| `"high"` \| `"critical"` \| `"unknown"`. |
| `nram.headroom_mb()` | MB still usable: min(system available, budget remaining), or `nil` if unknown. |
| `nram.info()` | Snapshot: `{total_mb, available_mb, used_mb, process_mb, limit_mb, pressure}`. |

## Budget gating

```niao
nram.set_limit_mb(8192)

if nram.ok(1024) {
    // safe to allocate ~1 GB batch buffer
} else {
    print("pressure:", nram.pressure(), "headroom:", nram.headroom_mb())
}

nram.set_limit_mb(0)   // restore unlimited
```

When both `set_limit_mb` and `set_limit_pct` are set, the effective budget is the **minimum** of the two.

## Errors

| Code | Meaning |
|------|---------|
| 2720 | Wrong argument count. |
| 2721 | Operation error — catchable `nram_error`. |
| 2722 | Wrong argument type. |
