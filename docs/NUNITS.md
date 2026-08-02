# nunits standard library

Physical units, quantity arithmetic, conversion, and dimensional checks. Native Rust implementation (~Python `pint` subset).

## Import

```niao
import "nunits"
```

Paths `import "std/nunits"` and `import "nunits"` are equivalent. Flat builtins (`nunits_parse`, `nunits_to`, …) are also available globally after import.

## Quick start

```niao
import "nunits"

let distance = nunits.parse("5.2 km")
let miles = nunits.to(distance, "mile")
print(nunits.to_string(miles))   // 3.231...

let speed = nunits.div(nunits.parse("100 km"), nunits.parse("2 h"))
let mph = nunits.to(speed, "mph")
print(nunits.to_string(mph))     // ~31.07 mph

let force = nunits.mul(nunits.parse("10 kg"), nunits.parse("9.8 m/s^2"))
print(nunits.dimension(force))   // kg*m/s^2

nunits.close(distance)
nunits.close(miles)
nunits.close(speed)
nunits.close(mph)
nunits.close(force)
```

## Quantity handles

Constructors return an integer **handle** on success, or a catchable `nunits_error` object on failure. Pass handles to arithmetic, conversion, and formatting functions. Call `nunits.close(handle)` when done (optional; handles are thread-local).

## Constructors

| Method | Description |
|--------|-------------|
| `nunits.quantity(magnitude, unit)` | Build from scalar + unit string (`"m"`, `"km/h"`, …). |
| `nunits.parse(s)` | Parse `"3.5 m"`, `"1.2e3 km"`, `"9.8 m/s^2"`. |
| `nunits.unit(s)` | Parse a unit expression without magnitude. |
| `nunits.valid_unit(s)` | `true` when `s` is a known/parseable unit. |
| `nunits.valid_quantity(s)` | `true` when `s` is a valid quantity literal. |
| `nunits.convert(mag, from, to)` | Scalar conversion; returns `float`. |
| `nunits.define(name, expr)` | Register custom unit alias (`"lightyear", "9.46e15 m"`). |
| `nunits.reset()` | Restore default registry and clear handles. |
| `nunits.close(handle)` | Free handle; returns `true` if it existed. |

## Introspection

| Method | Description |
|--------|-------------|
| `nunits.magnitude(q)` | Numeric value in the quantity's current unit. |
| `nunits.unit_of(q)` | Unit symbol string (e.g. `"km"`). |
| `nunits.dimension(q)` | Dimension string (e.g. `"m/s^2"`). |
| `nunits.dimensionless(q)` | `true` when dimensionless. |
| `nunits.compatible(a, b)` | `true` when dimensions match. |
| `nunits.to_string(q, precision?)` | Human-readable quantity. |
| `nunits.as_float(q)` | Extract scalar; catchable error if not dimensionless. |
| `nunits.definitions()` | Sorted list of registered unit names. |
| `nunits.prefixes()` | SI prefix names and symbols. |

## Conversion

| Method | Description |
|--------|-------------|
| `nunits.to(q, unit)` | Convert to another compatible unit; new handle. |
| `nunits.to_base(q)` | Express in SI base units for the dimension. |

## Arithmetic

| Method | Description |
|--------|-------------|
| `nunits.add(a, b)` / `sub` | Same-dimension only; catchable dimension error. |
| `nunits.mul(a, b)` / `div` | Combine dimensions (`m * m` → `m^2`). |
| `nunits.pow(q, exp)` | Integer exponent on magnitude and dimension. |
| `nunits.sqrt(q)` | Square root (halves dimension exponents). |
| `nunits.scale(q, factor)` | Multiply magnitude by dimensionless scalar. |
| `nunits.neg(q)` / `abs(q)` | Sign operations on magnitude. |
| `nunits.compare(a, b)` | `-1`, `0`, or `1`; dimension mismatch is catchable. |

## Constants

String unit symbols on the module object: `nunits.METER` (`"m"`), `SECOND`, `KILOGRAM`, `KELVIN`, `NEWTON`, `PASCAL`, `JOULE`, `WATT`, `HERTZ`.

## Built-in units (sample)

SI base and derived: `m`, `kg`, `s`, `A`, `K`, `mol`, `cd`, `N`, `Pa`, `J`, `W`, `Hz`, `L`.

Prefixes: `k`, `M`, `m`, `u`, `n`, … and long names (`kilo`, `mega`, …).

Common imperial/US: `inch`, `ft`, `mile`, `lb`, `oz`, `mph`, `psi`, `hp`.

Temperature (affine): `K`, `degC`, `degF`.

## Errors

| Code | Meaning |
|------|---------|
| 4600 | Wrong argument count. |
| 4601 | Semantic error (invalid handle, division by zero, …). |
| 4602 | Type mismatch (expected handle/string/number). |
| 4603 | Parse error (unknown unit, malformed expression). |
| 4604 | Dimension mismatch (incompatible units). |

Arity/type mistakes raise `RuntimeError`. Domain failures return catchable `nunits_error` values usable with `ntest.is_error` / `try`.

## See also

- [`ndecimal`](NDECIMAL.md) — arbitrary-precision decimals
- [`nmath`](NMATH.md) — scalar math
- [`ntest`](NTEST.md) — test harness
