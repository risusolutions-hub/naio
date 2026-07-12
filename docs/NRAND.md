# nrand standard library

Fast random numbers built on **xoshiro256\*\*** seeded via SplitMix64 — 32 bytes of state, ~1 ns per number, no locks. A thread-local default generator plus isolated seeded generator handles for reproducible runs.

## Import

```niao
import "nrand"
```

Paths `import "std/nrand"` and `import "nrand"` are equivalent. Flat builtins (`nrand_int`, `nrand_choice`, …) are also available globally after import.

## Quick start

```niao
import "nrand"

nrand.seed(42)                       // reproducible from here on
print(nrand.int(1, 6))               // dice roll, inclusive
print(nrand.float())                 // [0, 1)
print(nrand.choice(["a", "b", "c"]))
print(nrand.alphanum(16))            // e.g. q3fK9xL0pZw27aBn
```

## Default generator

| Method | Description |
|--------|-------------|
| `nrand.seed(n)` | Seed the thread-local generator (reproducible). Unseeded = time entropy. |
| `nrand.int(lo, hi)` | Uniform int in `[lo, hi]` inclusive, unbiased (Lemire rejection). |
| `nrand.float()` / `nrand.float(lo, hi)` | 53-bit uniform float in `[0,1)` or `[lo,hi)`. |
| `nrand.bool(p?)` | `true` with probability `p` (default `0.5`). |
| `nrand.bytes(n)` | `ByteArray` of `n` random bytes (≤ 16 MiB). |
| `nrand.hex(n)` | Random lowercase hex string of length `n`. |
| `nrand.alphanum(n)` | Random `[A-Za-z0-9]` string. |
| `nrand.string(n, charset)` | Random string drawn from `charset` chars. |

## Collections

| Method | Description |
|--------|-------------|
| `nrand.choice(arr)` | Random element (arrays, IntArray, FloatArray, StringArray). |
| `nrand.weighted(items, weights)` | Weighted choice; weights ≥ 0, sum > 0. |
| `nrand.shuffle(arr)` | In-place Fisher–Yates; returns the same array. |
| `nrand.sample(arr, k)` | `k` distinct elements, without replacement. |

## Distributions

| Method | Description |
|--------|-------------|
| `nrand.normal()` / `nrand.normal(mu, sigma)` | Gaussian (Box–Muller with spare caching). |
| `nrand.exponential(lambda)` | Exponential distribution. |

## Seeded generator handles

Isolated streams that don't disturb the default generator — ideal for reproducible simulations:

```niao
import "nrand"

let g = nrand.new_gen(1234)
let a = nrand.gen_int(g, 0, 99)
let x = nrand.gen_float(g)
let z = nrand.gen_normal(g)
let bs = nrand.gen_bytes(g, 32)
nrand.close_gen(g)
```

Same seed ⇒ same sequence, on any platform.

## Errors

| Code | Meaning |
|------|---------|
| 2620 | Wrong argument count. |
| 2621 | Operation failed (empty choice, bad range, length too large). |
| 2622 | Type mismatch. |
| 2623 | Invalid or closed generator handle. |

Note: nrand is **not** cryptographically secure. Use `crypto` for keys, tokens, and anything security-sensitive.
