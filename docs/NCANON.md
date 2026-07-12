# ncanon standard library

Deterministic canonicalization of Niao values to a compact JSON-like string (sorted object keys), plus stable FNV-1a 64 hashing and structural equality via the canonical form. Std-only — no external crates.

## Import

```niao
import "ncanon"
```

Paths `import "std/ncanon"` and `import "ncanon"` are equivalent. Flat builtins (`ncanon_canon`, `ncanon_hash`, …) are also available globally after import.

## Quick start

```niao
import "ncanon"

let a = {b: 2, a: 1}
let b = {a: 1, b: 2}

print(ncanon.canon(a))           // {"a":1,"b":2}
print(ncanon.equal(a, b))        // true
print(ncanon.hash(a))            // 16-char lowercase hex
print(ncanon.fingerprint(a))     // first 8 hex chars of hash
```

## Canonical form

| Value | Encoding |
|-------|----------|
| `nil` | `null` |
| `true` / `false` | `true` / `false` |
| int / bigint / float | Decimal digits (JSON-like; finite floats only) |
| string | Double-quoted with JSON escapes (`\"`, `\\`, `\n`, `\r`, `\t`, `\uXXXX` for other controls) |
| array | Compact `[…]` with `,` separators (no spaces) |
| object | Compact `{…}` with keys sorted lexicographically |

Packed arrays (`int_array`, `float_array`, …) encode the same as ordinary arrays. Functions, handles, and other non-data types are rejected.

## Functions

| Method | Description |
|--------|-------------|
| `ncanon.canon(value)` | Canonical JSON-like string. |
| `ncanon.hash(value)` | Lowercase 16-char hex of FNV-1a 64 over the UTF-8 bytes of `canon(value)`. |
| `ncanon.equal(a, b)` | `true` when `canon(a) == canon(b)` (key order independent). |
| `ncanon.fingerprint(value)` | First 8 hex characters of `hash(value)`. |

## Errors

| Code | Meaning |
|------|---------|
| 3070 | Wrong argument count. |
| 3071 | Nesting too deep (catchable `error` value). |
| 3072 | Unsupported / non-finite value (hard type error). |
