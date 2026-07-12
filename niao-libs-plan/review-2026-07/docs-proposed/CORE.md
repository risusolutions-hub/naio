# core builtins

Always-available builtins — no `import` needed. Registered in `crates/niao_runtime/src/lib.rs`
(≈17 core builtins). This is a stub drawn from the runtime registration; verify the full list and
signatures against source before publishing.

## Output & inspection

| Builtin | Description |
|---|---|
| `print(...)` | Print values (space-separated) + newline. |
| `len(x)` | Length of a string, array, or object. |
| `type(x)` | Type name as a string (`"int"`, `"float"`, `"string"`, `"array"`, `"object"`, `"error"`, `"nil"`, …). |

## Conversion

| Builtin | Description |
|---|---|
| `int(x)` | Parse/convert to integer. |
| `float(x)` | Parse/convert to float. |
| `bool(x)` | Truthiness / parse to boolean. |

## Errors & assertions

| Builtin | Description |
|---|---|
| `assert(cond, msg?)` | Raise if `cond` is false. |
| `error(code, msg)` | Construct a catchable error value. |
| `is_error(x)` | Test whether a value is an error. |

## Also in core (verify)

Timing (`time_ms`/elapsed), array helpers (push/pop/range), and iteration helpers are registered in
`lib.rs`. Enumerate the full `builtin_table` to complete this doc.

## v0.2.4 notes

- Add `dbg(x)` (print + return `x`), `todo()`, `unreachable()`, `identity(x)`, int `clamp`.
- Split `lib.rs` (1,708 LOC) registration into `builtins/*.rs` modules for maintainability.
- Ensure `len`/`type`/index are `#[inline]` and avoid a redundant `RefCell` borrow on the VM hot path.

> **Status:** stub. `core`'s manifest was NUL-corrupted (fixed in `../manifest-fixes/core__package.json`).
