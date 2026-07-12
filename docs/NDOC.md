# ndoc standard library

Extract and run doc-comment doctests from Niao source. Code lines use `// >>>` (or `/// >>>`); optional expected values use `// =>`.

## Import

```niao
import "ndoc"
```

Paths `import "std/ndoc"` and `import "ndoc"` are equivalent. Flat builtins (`ndoc_extract`, `ndoc_run`, …) are also available globally after import.

## Quick start

```niao
import "ndoc"

let source = """
// >>> 1 + 2
// => 3
// >>> let x = 10
// >>> x / 2
// => 5
"""

let blocks = ndoc.extract(source)
print(len(blocks))                 // 2

let result = ndoc.run(source)
print(result.ok, result.passed)    // true 2

let checked = ndoc.check(source)   // catchable error if any block fails
```

## Doctest syntax

```niao
// >>> 2 + 2
// => 4

/// >>> let n = 3
/// >>> n * n
/// => 9
```

- Each `// >>>` starts a new example block.
- Following lines without a new `>>>` continue the same block (use `//` prefix).
- `// =>` sets the expected value (parsed as a Niao expression).
- Without `=>`, the block only checks that execution succeeds.

## Functions

| Method | Description |
|--------|-------------|
| `ndoc.extract(source)` | Return an array of `{line, code: [strings], expect?}`. |
| `ndoc.run(source)` | Execute all blocks. Returns `{total, passed, failed, ok, results: [{line, code, got?, ok, message?}]}`. |
| `ndoc.check(source)` | Like `run`, but returns catchable `error` when `ok` is false or no doctests are found. |

Supported doctest code: expressions, `let` bindings, assignment to names, and `return`. Multi-line blocks share a temporary environment.

## Errors

| Code | Meaning |
|------|---------|
| 3210 | Wrong argument count. |
| 3211 | Doctest failure or empty source (catchable). |
| 3212 | Type / unsupported construct error. |
