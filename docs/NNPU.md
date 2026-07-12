# nnpu standard library

Best-effort NPU (neural accelerator) detection and budget mirror. Detects Apple Neural Engine, Intel AI Boost, Qualcomm Hexagon, AMD Ryzen AI, and Linux `/dev/accel` devices. When no NPU exists the API reports `available() == false` so programs can fall back to GPU or CPU (see `ndevice.best_device()`).

## Import

```niao
import "nnpu"
```

Paths `import "std/nnpu"` and `import "nnpu"` are equivalent. Flat builtins (`nnpu_available`, `nnpu_ok`, …) are also available globally after import.

## Quick start

```niao
import "nnpu"

if nnpu.available() {
    print(nnpu.vendor(), nnpu.name())
    print(nnpu.ok())          // false when globally throttled
} else {
    print("no NPU — use ndevice.best_device()")
}

let info = nnpu.info()
print(info.available, info.vendor, info.note)
```

## Functions

| Method | Description |
|--------|-------------|
| `nnpu.available()` | `true` when an NPU is detected on this machine. |
| `nnpu.vendor()` | Vendor string (e.g. `"Apple"`, `"Intel"`, `"none"`). |
| `nnpu.name()` | Device name or descriptive label. |
| `nnpu.info()` | Object: `{available, vendor, name, note, limit_pct}`. `note` explains detection method or absence. |
| `nnpu.set_limit(pct)` | Advisory NPU budget (`1..=100`) consulted by `ndevice.best_device()`. Returns `nil`. |
| `nnpu.get_limit()` | Current limit percent (`int`). |
| `nnpu.ok()` | `true` when NPU is present and global throttle level &lt; 2. |

## Fallback pattern

```niao
import "ndevice"

let dev = ndevice.best_device("infer")   // "npu" | "gpu" | "cpu"
if dev == "npu" && nnpu.ok() {
    // schedule on NPU
} else if dev == "gpu" {
    // GPU path
} else {
    // CPU path
}
```

## Errors

| Code | Meaning |
|------|---------|
| 2730 | Wrong argument count. |
| 2731 | Operation error — catchable `nnpu_error`. |
| 2732 | Wrong argument type. |
