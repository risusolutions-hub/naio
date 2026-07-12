# nquota standard library

Token-bucket rate limiting with integer handles. Tokens refill continuously based on wall-clock `SystemTime` elapsed since the last update — no background threads.

## Import

```niao
import "nquota"
```

Paths `import "std/nquota"` and `import "nquota"` are equivalent. Flat builtins (`nquota_new`, `nquota_take`, …) are also available globally after import.

## Quick start

```niao
import "nquota"
import "time"

let q = nquota.new(10, 5)     // 10 tokens/sec, burst of 5
if nquota.take(q) {
    // allowed
} else {
    let ms = nquota.wait_ms(q)
    time.sleep_ms(ms)
    nquota.take(q)
}

print(nquota.stats(q))        // {tokens, rate, burst}
nquota.close(q)
```

## Creating limiters

| Method | Description |
|--------|-------------|
| `nquota.new(rate_per_sec, burst?)` | Create a token bucket. `rate_per_sec` is tokens per second (int or float, must be `> 0`). Optional `burst` is the maximum token balance (defaults to `max(rate, 1)`). Starts full. Returns an integer handle. |
| `nquota.close(handle)` | Free the bucket; returns `true` if the handle existed. |

## Operations

| Method | Description |
|--------|-------------|
| `nquota.take(h, n?)` | Try to consume `n` tokens (default `1`). Returns `true` on success; does not partially consume on failure. |
| `nquota.ok(h)` | `true` when at least 1 token is available (does not consume). |
| `nquota.wait_ms(h)` | Suggested milliseconds to wait until 1 token is available (`0` if already available). |
| `nquota.reset(h)` | Refill to full `burst` immediately. |
| `nquota.stats(h)` | `{tokens, rate, burst}` after applying pending refill. |

Refill runs lazily on every operation: elapsed seconds × `rate` are added, capped at `burst`.

## Errors

| Code | Meaning |
|------|---------|
| 3090 | Wrong argument count. |
| 3091 | Operation failed (non-positive rate/burst/`n`) — catchable `error`. |
| 3092 | Wrong argument type. |
| 3093 | Invalid or closed quota handle — catchable `error`. |
