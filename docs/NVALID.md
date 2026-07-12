# nvalid standard library

Declarative data validation: schema objects with type/required/range/length/pattern/one_of rules plus fast hand-rolled checks (email, URL, UUID, IPv4). Friendly `{ok, errors}` results.

## Import

```niao
import "nvalid"
```

Paths `import "std/nvalid"` and `import "nvalid"` are equivalent. Flat builtins (`nvalid_check`, `nvalid_is_email`, …) are also available globally after import.

## Quick start

```niao
import "nvalid"

let user = {name: "vivek", age: 27, email: "vivek@example.com"}

let schema = {
    name:  {type: "string", required: true, min_len: 2, max_len: 60},
    age:   {type: "int", min: 0, max: 150},
    email: {type: "string", required: true, email: true},
    role:  {one_of: ["admin", "user", "guest"]}
}

let r = nvalid.check(user, schema)
if !r.ok { for e in r.errors { print(e) } }
```

## Rules

Each schema key maps a field name to a rule object:

| Rule | Applies to | Description |
|------|-----------|-------------|
| `type` | any | `string`, `int`, `float`, `number`, `bool`, `array`, `object`, `nil`. |
| `required` | any | Missing or `nil` field fails. Optional fields skip all checks when absent. |
| `min` / `max` | numbers | Inclusive numeric bounds. |
| `min_len` / `max_len` | strings, arrays, objects | Length bounds (chars for strings). |
| `one_of` | any | Value must deep-equal one of the listed values. |
| `pattern` | strings | Regex match (compiled + cached via the `re` engine). |
| `email` / `url` / `uuid` / `ipv4` | strings | Built-in fast checks. |
| `non_blank` | strings | Rejects empty/whitespace-only. |

Validating a non-object value applies the schema as a single rule: `nvalid.check(5, {type: "int", min: 1})`.

## Functions

| Method | Description |
|--------|-------------|
| `nvalid.check(value, schema)` | `{ok: bool, errors: ["field: message", ...]}`. |
| `nvalid.assert(value, schema)` | Returns `value` on success, catchable `error` listing problems on failure. |
| `nvalid.is_email(s)` / `is_url(s)` / `is_uuid(s)` / `is_ipv4(s)` | Fast boolean checks. |
| `nvalid.is_int_str(s)` / `is_float_str(s)` | Numeric-string checks. |
| `nvalid.matches(s, pattern)` | One-off regex test (pattern cached). |

## Errors

| Code | Meaning |
|------|---------|
| 2680 | Wrong argument count. |
| 2681 | Validation failed (from `assert`, catchable). |
| 2682 | Invalid schema (hard error — programmer mistake). |
