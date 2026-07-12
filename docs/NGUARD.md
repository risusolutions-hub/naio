# nguard standard library

PII scan and redact (email, phone, SSN, credit card with Luhn, IP, API keys) plus thread-local denylist middleware hooks for prompt/response filtering.

## Import

```niao
import "nguard"
```

Paths `import "std/nguard"` and `import "nguard"` are equivalent. Flat builtins (`nguard_scan`, `nguard_redact`, …) are also available globally after import.

## Quick start

```niao
import "nguard"

let text = "Contact alice@example.com or 555-123-4567. Key: sk-test1234567890abcdef"
print(nguard.scan(text))
print(nguard.redact(text))
print(nguard.has_pii(text))

nguard.denylist_add("forbidden")
print(nguard.filter(text))   // redacts PII, then checks denylist
```

Run: `niao run examples/nguard_demo.niao`

## Functions

| Method | Description |
|--------|-------------|
| `nguard.scan(text, types?)` | Returns `{count, findings}` where each finding is `{type, start, end, match}`. |
| `nguard.redact(text, opts?)` | Replace detected PII. `opts.replacement` (default `[REDACTED]` → `[email]`, `[phone]`, …). `opts.types` limits categories. |
| `nguard.has_pii(text, types?)` | `true` when any configured PII type is present. |
| `nguard.denylist_add(pattern)` | Add a case-insensitive substring to the thread-local denylist. |
| `nguard.denylist_remove(pattern)` | Remove a pattern; returns `true` if it existed. |
| `nguard.denylist_clear()` | Clear all denylist patterns. |
| `nguard.denylist_check(text)` | Returns `{blocked, matches}`. |
| `nguard.filter(text, opts?)` | Redact PII (default on), then enforce denylist (default block). Returns catchable error when blocked. |

### PII types

`email`, `phone`, `ssn`, `card` (Luhn-validated), `ip` (IPv4/IPv6), `api_key` (common `sk-`, `Bearer`, `AKIA`, `api_key=` prefixes).

## Errors

| Code | Meaning |
|------|---------|
| 3320 | Wrong argument count. |
| 3321 | Empty denylist pattern, blocked text, or other semantic error (catchable). |
| 3322 | Wrong argument type or unknown PII type name. |
