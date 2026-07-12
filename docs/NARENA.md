# narena — Pooled Buffer Arena

`narena` reduces allocation churn by handing out reusable `byte_array` buffers from a thread-local pool. Return buffers with `recycle`; `reset` clears outstanding counts without shrinking the pool.

Import with:

```niao
import "narena"
// or
import "std/narena"
```

---

## Quick start

```niao
import "narena"

let a = narena.new(4096, 8)     // block_size, pool_cap
let buf = narena.alloc(a, 1024)
// ... use buf ...
narena.recycle(a, buf)
narena.reset(a)
print(narena.stats(a))
narena.close(a)
```

---

## Functions

| Method | Description |
|--------|-------------|
| `narena.new(block_size?, pool_cap?)` | Create an arena (defaults: 4096, 16). Returns handle. |
| `narena.alloc(handle, size)` | Allocate a zero-filled `byte_array` of at least `size` bytes. |
| `narena.recycle(handle, byte_array)` | Return a buffer to the pool (up to `pool_cap`). |
| `narena.reset(handle)` | Mark all allocations released; keep pooled buffers. |
| `narena.stats(handle)` | `{block_size, pool_cap, pooled, outstanding, total_allocated, total_recycled, reset_count}`. |
| `narena.close(handle)` | Destroy the arena. Returns `true` if it existed. |

---

## Errors

| Code | Meaning |
|------|---------|
| 3370 | Wrong argument count. |
| 3371 | Invalid size/capacity — catchable `narena_error`. |
| 3372 | Wrong argument type. |
| 3373 | Invalid or closed handle — catchable `narena_error`. |
