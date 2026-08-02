# ndecimal standard library

Arbitrary-precision decimals and exact rationals with money-safe rounding modes. Native implementation on [`niao_bignum`](../crates/niao_bignum) — Python `decimal` + `fractions` subset.

## Import

```niao
import "ndecimal"
```

Paths `import "std/ndecimal"` and `import "ndecimal"` are equivalent. Flat builtins (`ndecimal_decimal`, `ndecimal_add`, …) are also available globally after import.

## Quick start

```niao
import "ndecimal"

ndecimal.context(28, "half_even")

let price = ndecimal.decimal("19.995")
let tax_rate = ndecimal.decimal("0.0825")
let tax = ndecimal.mul(tax_rate, price)
let total = ndecimal.round_money(ndecimal.add(price, tax))
print(ndecimal.to_string(total))   // 21.64

let third = ndecimal.fraction(1, 3)
let approx = ndecimal.to_decimal(third)
print(ndecimal.to_string(approx))  // 0.3333333333333333333333333333
```

## Decimal handles

`ndecimal.decimal(value)` parses a string or coerces a number and returns an opaque **handle** (`int`). Pass handles (or strings that auto-parse) to arithmetic and formatting functions.

## Context

| Method | Description |
|--------|-------------|
| `ndecimal.context(prec?, rounding?)` | Set thread-local precision (default 28) and rounding mode; returns `{prec, rounding}`. |
| `ndecimal.get_context()` | Read current context. |

Rounding mode names: `half_even` (default, banker's), `half_up`, `half_down`, `up`, `down`, `ceiling`, `floor`, `05up`. Constants `ndecimal.ROUND_HALF_EVEN`, etc. hold the string name.

## Decimal functions

| Method | Description |
|--------|-------------|
| `ndecimal.decimal(s)` | Parse/create decimal; catchable `error` on invalid input. |
| `ndecimal.valid_decimal(s)` | `true` when `s` is a valid decimal literal. |
| `ndecimal.add(a, b)` / `sub` / `mul` / `div` / `mod` | Arithmetic under current context. |
| `ndecimal.pow(d, exp)` | Integer exponent. |
| `ndecimal.abs(d)` / `neg(d)` | Sign operations. |
| `ndecimal.compare(a, b)` | `-1`, `0`, or `1`; catchable `error` on NaN. |
| `ndecimal.quantize(d, exp, rounding?)` | Round to exponent (e.g. `-2` → cents). |
| `ndecimal.round_money(d, places?, rounding?)` | Quantize to `places` decimal digits (default 2) with money context. |
| `ndecimal.normalize(d)` | Strip trailing coefficient zeros. |
| `ndecimal.to_integral(d, rounding?)` | Round to integer. |
| `ndecimal.sqrt(d)` | Square root to context precision. |
| `ndecimal.to_string(d)` / `to_sci(d)` / `to_eng(d)` | Formatting. |
| `ndecimal.as_tuple(d)` | `{sign, coeff, exp}` decomposition. |
| `ndecimal.is_zero(d)` / `is_finite` / `is_nan` / `is_inf` | Predicates. |
| `ndecimal.from_float(f)` | Build from `float` using its exact decimal repr string. |

## Fraction functions

| Method | Description |
|--------|-------------|
| `ndecimal.fraction(n, d?)` | Exact rational; `d` defaults to `1`. |
| `ndecimal.valid_fraction(s)` | `true` for `"n"` or `"n/d"` forms. |
| `ndecimal.numer(f)` / `denom(f)` | Numerator/denominator as decimal strings. |
| `ndecimal.frac_add(a, b)` / `frac_mul` / `frac_div` | Exact rational arithmetic. |
| `ndecimal.limit_denominator(f, max?)` | Best rational with denominator ≤ `max` (default 10000). |
| `ndecimal.to_decimal(f)` | Convert to decimal under current context. |

## Errors

| Code | Meaning |
|------|---------|
| 4100 | Wrong argument count. |
| 4101 | Semantic error (division by zero, invalid op, catchable). |
| 4102 | Type mismatch (hard error). |
| 4103 | Parse error (catchable). |

## Deferred / not yet implemented

- `decimal` context traps (`InvalidOperation`, `Overflow` as signals)
- Transcendentals `exp`, `ln`, `log10` on decimals
- `compare_total` / signaling NaN distinctions beyond basic NaN checks
- Full `decimal` module global context inheritance across threads
