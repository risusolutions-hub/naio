# nfuzz standard library

Deterministic property / fuzz helpers backed by a thread-local **xorshift64** RNG. Seed once, then generate ints, floats, strings, bytes, and shuffled copies for reproducible test cases.

## Import

```niao
import "nfuzz"
```

Paths `import "std/nfuzz"` and `import "nfuzz"` are equivalent. Flat builtins (`nfuzz_seed`, `nfuzz_int`, …) are also available globally after import.

## Quick start

```niao
import "nfuzz"

nfuzz.seed(42)
print(nfuzz.int(0, 100))
print(nfuzz.float(0.0, 1.0))
print(nfuzz.bool())
print(nfuzz.string(12))
print(nfuzz.pick(["a", "b", "c"]))
print(nfuzz.shuffle([1, 2, 3, 4]))   // new array; original unchanged
print(nfuzz.bytes(8))
print(nfuzz.cases(5, 1, 10))          // five ints in 1..=10
```

## Functions

| Method | Description |
|--------|-------------|
| `nfuzz.seed(n)` | Reseed the thread-local RNG. Same seed → same subsequent sequence. |
| `nfuzz.int(min, max)` | Inclusive uniform integer in `min..=max`. Catchable error if `min > max`. |
| `nfuzz.float(min, max)` | Uniform float in `[min, max)`. Accepts int or float bounds. Catchable error if `min > max`. |
| `nfuzz.bool()` | Random boolean (fair coin). |
| `nfuzz.string(len?)` | Alphanumeric string of length `len` (default `8`). Catchable error if `len < 0` or too large. |
| `nfuzz.pick(array)` | One element chosen uniformly. Catchable error on empty array. Accepts `Array` / `IntArray` / `FloatArray` / `StringArray`. |
| `nfuzz.shuffle(array)` | Returns a **new** shuffled array copy (Fisher–Yates). Original is unchanged. |
| `nfuzz.bytes(n)` | `ByteArray` of `n` bytes (each `0..=255`). |
| `nfuzz.cases(n, min, max)` | `IntArray` of `n` integers each in `min..=max`. |

## Determinism

All generators share one thread-local xorshift64 state. After `nfuzz.seed(s)`, the sequence of draws is fixed for that thread. There are no generator handles — only the global RNG.

## Errors

| Code | Meaning |
|------|---------|
| 3110 | Wrong argument count. |
| 3111 | Catchable semantic error (empty pick, bad range, negative length). |
| 3112 | Wrong argument type. |
| 3113 | Reserved (invalid handle — unused; global RNG only). |
