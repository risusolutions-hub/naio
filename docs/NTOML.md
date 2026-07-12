# ntoml standard library

Parse and stringify [TOML](https://toml.io/) configuration text. Uses the fast `niao_json_core::toml` parser and a minimal built-in emitter for objects, nested tables, array-of-tables, and inline arrays.

## Import

```niao
import "ntoml"
```

Paths `import "std/ntoml"` and `import "ntoml"` are equivalent. Flat builtins (`ntoml_parse`, `ntoml_stringify`, …) are also available globally after import.

## Quick start

```niao
import "ntoml"

let cfg = ntoml.parse("""
[server]
host = "127.0.0.1"
port = 3001
""")

print(cfg.server.host)
print(ntoml.stringify(cfg))
```

## Functions

| Method | Description |
|--------|-------------|
| `ntoml.parse(text)` | Parse TOML string → object (nested tables become nested objects). |
| `ntoml.parse_file(path)` | Read a file and parse its contents. |
| `ntoml.stringify(value)` | Encode a value as compact TOML text. |
| `ntoml.stringify_pretty(value)` | Same as `stringify`, with blank lines between sections. |

## Supported values

**Parse** returns Niao values: objects, arrays, strings, ints, floats, and bools. `[[array-of-tables]]` headers become arrays of objects.

**Stringify** accepts:

- Top-level **objects** with scalar fields, inline arrays, nested `[table]` sections, and `[[array-of-tables]]`.
- Scalar types: string, int, float, bool.
- Inline arrays of scalars (`IntArray`, `StringArray`, etc. are supported).

`nil`, functions, and other non-TOML types raise a type error.

## Errors

| Code | Meaning |
|------|---------|
| 2840 | Wrong argument count. |
| 2841 | I/O or general TOML error (e.g. unreadable file). |
| 2842 | Type error (unsupported value for stringify). |
| 2843 | TOML parse error. |

## Example

See `examples/ntoml_demo.niao`.
