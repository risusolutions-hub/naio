# nlint standard library

AST-as-data linting via `niao_parser`: parse Niao source into inspectable objects, run built-in or data-driven rules, and collect diagnostics with `nlint.check(source)`.

## Import

```niao
import "nlint"
```

Paths `import "std/nlint"` and `import "nlint"` are equivalent. Flat builtins (`nlint_parse`, `nlint_check`, …) are also available globally after import.

## Quick start

```niao
import "nlint"

let ast = nlint.parse("fn add(a: int, b: int) -> int { return a + b }")
print(ast.kind)                    // Program

let report = nlint.check(source)
if !report.ok {
    for issue in report.issues {
        print(issue.rule, issue.line, issue.message)
    }
}
```

## Functions

| Method | Description |
|--------|-------------|
| `nlint.parse(source)` | Parse `source` and return AST-as-data (`{kind, span, …}` tree). Catchable `error` on parse failure. |
| `nlint.check(source, rules?)` | Lint `source`. Uses default rules when `rules` is omitted. Returns `{ok, issues: [{rule, message, line, col, severity}]}`. Parse errors are catchable `error` values. |
| `nlint.rules()` | Default data-driven rule objects (`no-empty-fn`, `no-print`, `require-main`). |

## Default rules

| Rule | Severity | Meaning |
|------|----------|---------|
| `no-empty-fn` | warn | Function body has no statements. |
| `no-print` | warn | `print()` inside a function. |
| `no-top-level-print` | warn | Top-level `print()` call. |
| `require-main` | warn | No `fn main()` entry point. |

## Custom / data-driven rules

Pass an array of rule objects as the second argument to `check`:

```niao
let rules = [
    {id: "no-debug", on: "Call", callee: "debug", severity: "warn"},
    {id: "no-empty-fn", on: "Fn", check: "empty_body"},
    {id: "custom", on: "Fn", fn: my_rule_fn}
]
let report = nlint.check(source, rules)
```

- **Data rules:** `on` is `Fn`, `Call`, or `Program`; optional `check`, `callee`, `severity`.
- **Custom rules:** include `fn` — called with each matching AST node (as data). Return `true` to flag, or a non-empty `string` / `error` with a message.

## Errors

| Code | Meaning |
|------|---------|
| 3220 | Wrong argument count. |
| 3221 | Lint semantic error (catchable). |
| 3222 | Type error (wrong argument types). |
| 3223 | Parse error (catchable). |
