# nshape standard library

Describe and check value shapes: structural `of` strings, array `rank`/`dims`, shape equality via `match`, and simple `check` against a type name or schema object.

## Import

```niao
import "nshape"
```

Paths `import "std/nshape"` and `import "nshape"` are equivalent. Flat builtins (`nshape_of`, `nshape_check`, …) are also available globally after import.

## Quick start

```niao
import "nshape"

print(nshape.of(42))                    // int
print(nshape.of([1, 2, 3]))             // array[3]
print(nshape.of({name: "a", age: 1}))   // {age: int, name: string}

print(nshape.rank([1, 2]))              // 1
print(nshape.dims([1, 2, 3]))           // [3]

print(nshape.match([1, 2], ["a", "b"])) // true (both array[2])

let r = nshape.check({name: "x", age: 20}, {name: "string", age: "int"})
print(r.ok)                             // true
```

## Shape strings

`nshape.of(value)` returns a compact description:

| Value | Example |
|-------|---------|
| Scalars | `int`, `bigint`, `float`, `string`, `bool`, `nil` |
| Generic array | `array[3]` |
| Packed arrays | `int_array[10]`, `float_array[10]`, `bool_array[n]`, `byte_array[n]`, `string_array[n]` |
| Object | `{age: int, name: string}` (keys sorted) |

## Functions

| Method | Description |
|--------|-------------|
| `nshape.of(value)` | Shape string for `value`. |
| `nshape.rank(arr)` | `1` for any array (generic or packed), else `0`. |
| `nshape.dims(arr)` | `[len]` for arrays, else `[]`. |
| `nshape.match(a, b)` | `true` when `of(a) == of(b)`. |
| `nshape.check(value, expected)` | `{ok: bool, errors: [...]}`. |

### check

`expected` may be:

- A **type name**: `int`, `float`, `string`, `bool`, `nil`, `object`, `array`, `number`, `int_array`, `float_array`, …
- An **exact shape** from `of()` (e.g. `array[3]`, `float_array[10]`)
- A **schema object** whose values are type strings: `{name: "string", age: "int"}`

Missing schema keys and type mismatches produce friendly error messages.

## Errors

| Code | Meaning |
|------|---------|
| 3120 | Wrong argument count. |
| 3121 | Invalid schema (e.g. non-string schema value). |
| 3122 | Type mismatch on arguments (e.g. `check` expected string/object). |
