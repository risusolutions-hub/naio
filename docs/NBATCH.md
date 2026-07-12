# nbatch standard library

Adaptive batch sizing for memory-aware training and inference loops. Suggest a batch from VRAM/RAM budget, compute step counts, clamp/scale sizes, and halve on OOM-style failures.

## Import

```niao
import "nbatch"
```

Paths `import "std/nbatch"` and `import "nbatch"` are equivalent. Flat builtins (`nbatch_suggest`, `nbatch_fit`, …) are also available globally after import.

## Quick start

```niao
import "nbatch"

// Prefer VRAM when set; else RAM; else assume 1024 MiB.
// Uses half of available memory as the working budget.
let bs = nbatch.suggest(8192, nil, 4 * 1024 * 1024)  // 1024
let steps = nbatch.fit(10000, bs)

let next = nbatch.halve_on(false, bs)   // 512 after a failed step
let capped = nbatch.clamp(next, 1, 256)
let grown = nbatch.scale(capped, 1.5)
```

## Formula

`nbatch.suggest(vram_mb?, ram_mb?, item_bytes?, max?)`:

1. `available_mb = vram_mb` if set and &gt; 0, else `ram_mb` if set and &gt; 0, else `1024`
2. `batch = floor((available_mb * 1024 * 1024 * 0.5) / item_bytes)`
3. Clamp to `[1, max]` where `max` defaults to `4096` and `item_bytes` defaults to `1`

Missing trailing args and `nil` both mean “unset”.

## Functions

| Method | Description |
|--------|-------------|
| `nbatch.suggest(vram_mb?, ram_mb?, item_bytes?, max?)` | Suggested batch size (`int`). Arity `0..4`. |
| `nbatch.fit(total, batch)` | `ceil(total / batch)` step count (`0` when `total` is `0`). |
| `nbatch.clamp(n, min, max)` | Clamp `n` into `[min, max]`. |
| `nbatch.scale(n, factor)` | `trunc(n * factor)` as an int. |
| `nbatch.halve_on(ok_bool, n)` | Returns `n` when `ok_bool` is true; otherwise `max(1, n / 2)`. |

## Errors

| Code | Meaning |
|------|---------|
| 3030 | Wrong argument count. |
| 3031 | Invalid numeric domain (e.g. `item_bytes <= 0`, `batch <= 0`, `min > max`). |
| 3032 | Wrong argument type. |
