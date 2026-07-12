# nexplain — actionable error enrichment

Turn raw error messages (or Error values) into structured hints with suggested fixes. Built-in patterns cover common Niao failures; custom rules can be registered at runtime.

## Import

```niao
import "nexplain"
```

Paths `import "std/nexplain"` and `import "nexplain"` are equivalent. Flat builtins (`nexplain_of`, `nexplain_format`, …) are also available globally after import.

## Quick start

```niao
import "nexplain"

let info = nexplain.of("undefined variable 'count'")
print(info.message)   // original text
print(info.hint)      // what it usually means
print(info.fix)       // suggested next step

print(nexplain.format("division by zero"))
```

## Functions

| Method | Description |
|--------|-------------|
| `nexplain.of(msg_or_error)` | Enrich a string or Error → `{message, hint, fix, code?}`. `code` is present only when the argument is an Error value. |
| `nexplain.register(pattern, hint, fix?)` | Add a custom substring rule (checked before builtins). Returns `true`. |
| `nexplain.hints()` | Array of `{pattern, hint, fix}` — custom rules first, then builtins. |
| `nexplain.format(msg_or_error)` | Pretty multi-line string (`Message` / `Hint` / `Fix` / optional `Code`). |
| `nexplain.clear_custom()` | Remove all user rules; builtins stay. Returns `true`. |

Matching is case-insensitive substring search on the message. The first matching custom rule wins; otherwise the first matching builtin.

## Built-in patterns

| Pattern | Typical meaning |
|---------|-----------------|
| `undefined` | Name used before definition |
| `arity` | Wrong argument count |
| `type` | Wrong value type |
| `import` | Module import failed |
| `division` | Invalid / zero division |
| `nil` | Unexpected nil |
| `handle` | Invalid or closed handle |
| `permission` / `denied` | Access denied |
| `timeout` | Operation timed out |

Unmatched messages get a generic hint and fix.

## Custom rules

```niao
nexplain.register("quota", "Usage quota exceeded", "Wait for reset or raise the limit")
let info = nexplain.of("quota exceeded for project X")
print(info.hint)   // "Usage quota exceeded"
nexplain.clear_custom()
```

## Errors

| Code | Meaning |
|------|---------|
| 3010 | Wrong argument count. |
| 3011 | Logic error (e.g. empty `register` pattern). |
| 3012 | Wrong argument type. |

## See also

- `nvalid` — validate data before it becomes an error.
- `ntest` — `is_error` / assertions around Error values.
