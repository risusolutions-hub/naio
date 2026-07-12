# ncontract standard library

Design-by-contract helpers: preconditions (`require`), postconditions (`ensure`), soft checks (`check`), type asserts, and a small object invariant checker.

## Import

```niao
import "ncontract"
```

Paths `import "std/ncontract"` and `import "ncontract"` are equivalent. Flat builtins (`ncontract_require`, `ncontract_check`, …) are also available globally after import.

## Quick start

```niao
import "ncontract"

fn divide(a, b) {
    ncontract.require(b != 0, "divisor must be non-zero")
    let r = a / b
    ncontract.ensure(true, "unreachable")
    return r
}

let soft = ncontract.check(1 > 0)          // true
let soft_bad = ncontract.check(false, "no") // error value (catchable)

ncontract.assert_type(42, "int")

let r = ncontract.invariant(
    {name: "vivek", age: 27},
    {name: {required: true, type: "string"}, age: {type: "int"}}
)
print(r.ok)  // true
```

## Functions

| Method | Description |
|--------|-------------|
| `ncontract.require(cond, msg?)` | Throws `RuntimeError` (E3081) if `cond` is falsy. Returns `true` on success. Default message: `"precondition failed"`. |
| `ncontract.ensure(cond, msg?)` | Same as `require` for postconditions. Default message: `"postcondition failed"`. |
| `ncontract.check(cond, msg?)` | Returns `true` if truthy; otherwise a catchable `error` value (E3081). Default message: `"check failed"`. |
| `ncontract.assert_type(v, type_str)` | Throws (E3082) if `v` is not the named type. Returns `v` on success. |
| `ncontract.invariant(obj, rules)` | Soft object check → `{ok: bool, errors: [string, ...]}`. |

Conditions use Niao truthiness (`false`, `nil`, `0`, `""` are falsy).

## `assert_type` types

`type_str` must be one of:

| Type | Matches |
|------|---------|
| `int` | `int`, `bigint` |
| `float` | `float` |
| `string` | `string` |
| `bool` | `bool` |
| `nil` | `nil` |
| `array` | arrays (including packed arrays) |
| `object` | objects |
| `function` | user and native functions |

Unknown `type_str` values throw E3082.

## `invariant` rules

Each rules key is a field name mapped to a rule object. Supported keys (subset of `nvalid`):

| Rule | Description |
|------|-------------|
| `required` | Missing or `nil` field fails. |
| `type` | Same type names as `assert_type`. |

```niao
let r = ncontract.invariant(
    {age: "x"},
    {name: {required: true}, age: {type: "int"}}
)
// r.ok == false
// r.errors includes "name: is required" and "age: expected int, got string"
```

Invalid rule shapes (non-object rules, unknown `type` string) throw E3082.

## Errors

| Code | Meaning |
|------|---------|
| 3080 | Wrong argument count. |
| 3081 | Contract violated (`require` / `ensure` throw; `check` returns error value). |
| 3082 | Type mismatch or bad argument / rule type. |
