# nmath standard library

Scalar math, integer combinatorics, and descriptive statistics. Std-only native Rust. Stats functions accept plain arrays, `IntArray`, and `FloatArray` (packed arrays take a fast path).

## Import

```niao
import "nmath"
```

Paths `import "std/nmath"` and `import "nmath"` are equivalent. Flat builtins (`nmath_sqrt`, `nmath_mean`, …) are also available globally after import.

## Quick start

```niao
import "nmath"

print(nmath.sqrt(2.0))                  // 1.4142135623730951
print(nmath.pow(2, 10))                 // 1024 (int-preserving)
print(nmath.mean([1, 2, 3, 4, 5]))      // 3.0
print(nmath.percentile([1,2,3,4,5], 90))// 4.6
print(nmath.pi)                          // 3.141592653589793
```

## Constants (namespace only)

`nmath.pi`, `nmath.e`, `nmath.tau`, `nmath.inf`, `nmath.nan`

## Powers, logs, trig

| Method | Description |
|--------|-------------|
| `sqrt(x)` `cbrt(x)` | Square/cube root. |
| `pow(x, y)` | `int ^ int` stays int when it fits; else float. |
| `exp(x)` `ln(x)` `log2(x)` `log10(x)` `log(x, base)` | Exponentials and logarithms. |
| `sin cos tan asin acos atan atan2(y, x)` | Trigonometry (radians). |
| `sinh cosh tanh` | Hyperbolics. |
| `hypot(x, y)` | `sqrt(x² + y²)` without overflow. |
| `deg(rad)` / `rad(deg)` | Angle conversion. |

## Rounding & shaping

| Method | Description |
|--------|-------------|
| `floor(x)` `ceil(x)` `round(x)` `trunc(x)` | Return `int` (ints pass through). |
| `round_to(x, decimals)` | Round to `decimals` places, returns float. |
| `abs(x)` | Int-preserving absolute value. |
| `sign(x)` | `-1`, `0`, or `1`. |
| `clamp(x, lo, hi)` | Int-preserving when all args are ints. |
| `lerp(a, b, t)` | Linear interpolation. |
| `map_range(x, a0, a1, b0, b1)` | Remap `x` from one range to another. |
| `is_nan(x)` `is_finite(x)` `is_inf(x)` | Float predicates. |

## Integer combinatorics

| Method | Description |
|--------|-------------|
| `gcd(a, b)` / `lcm(a, b)` | Greatest common divisor / least common multiple. |
| `factorial(n)` | `n!` for `0..=20` (domain `error` beyond — use BigInt math). |
| `comb(n, k)` | Binomial coefficient (i128 intermediate, overflow → `error`). |
| `perm(n, k)` | Permutations. |

## min / max

`nmath.min(...)` and `nmath.max(...)` accept either variadic scalars (`nmath.max(3, 9, 4)`) or a single array (`nmath.min(values)`). Int-preserving when all inputs are ints.

## Statistics

All accept `[numbers]`, `IntArray`, or `FloatArray`.

| Method | Description |
|--------|-------------|
| `sum(arr)` | Int-preserving for IntArray (overflow promotes to float). |
| `mean(arr)` `median(arr)` `mode(arr)` | Central tendency. |
| `variance(arr, population?)` | Sample by default; pass `true` for population. |
| `stdev(arr, population?)` | Square root of variance. |
| `percentile(arr, p)` | Linear interpolation, `p` in `0..=100`. |

Empty inputs (and sample variance with < 2 values) return a catchable `error` value.

## Errors

| Code | Meaning |
|------|---------|
| 2610 | Wrong argument count. |
| 2611 | Operation failed. |
| 2612 | Type mismatch. |
| 2613 | Domain error (negative factorial, empty stats input, overflow). |
