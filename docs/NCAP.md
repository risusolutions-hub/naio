# ncap standard library

Cooperative capability sandbox. Scripts opt into a deny-by-default grant set for `net`, `fs`, `env`, `process`, `gpu`, and `all`.

**Cooperative only:** builtins and other stdlib modules do **not** auto-check capabilities. Call `ncap.require(cap)` (or `check`) around sensitive work yourself.

## Import

```niao
import "ncap"
```

Paths `import "std/ncap"` and `import "ncap"` are equivalent. Flat builtins (`ncap_grant`, `ncap_require`, …) are also available globally after import.

## Quick start

```niao
import "ncap"

print(ncap.enabled())   // false — allow_all by default

ncap.deny_all()
ncap.grant(["net", "fs"])

print(ncap.check("net"))   // true
print(ncap.check("env"))   // false

let ok = ncap.require("net")
print(ok)                  // true

let denied = ncap.require("env")
print(denied)              // catchable error (DENIED)

ncap.allow_all()           // unrestricted again
```

## Capabilities

| Cap | Intended use |
|-----|--------------|
| `net` | Network I/O |
| `fs` | Filesystem access |
| `env` | Environment variables / secrets |
| `process` | Subprocess / process control |
| `gpu` | GPU / accelerator use |
| `all` | Grants every capability while sandbox is enabled |

## Functions

| Method | Description |
|--------|-------------|
| `ncap.allow_all()` | Default mode: sandbox off, every check succeeds. Clears grants. |
| `ncap.deny_all()` | Enable sandbox and clear all grants. |
| `ncap.grant(cap_or_array)` | Grant one capability string or an array of them. |
| `ncap.revoke(cap_or_array)` | Revoke one capability or an array of them. |
| `ncap.list()` | Sorted array of currently granted capability strings. |
| `ncap.check(cap)` | `true` if allowed (sandbox off, or `cap` / `all` granted). |
| `ncap.require(cap)` | `true` if allowed; otherwise a catchable `error` (DENIED). |
| `ncap.enabled()` | `true` after `deny_all` (sandbox active); `false` after `allow_all`. |

## Semantics

1. **Default:** `allow_all` — `enabled()` is `false`, `check` / `require` always succeed.
2. **`deny_all`:** turns the sandbox on and empties the grant set.
3. **`grant("all")`:** while enabled, every `check` / `require` succeeds until `all` is revoked.
4. **State is thread-local** — each thread has its own grant set.

## Errors

| Code | Meaning |
|------|---------|
| 2980 | Wrong argument count. |
| 2981 | Unknown capability name (not one of the valid caps). |
| 2982 | Wrong argument type (expected string or array of strings). |
| 2983 | Capability denied — catchable `ncap_error` from `require`. |

## Notes

- This is a policy helper for scripts and libraries, not a hard OS sandbox.
- Prefer `require` at trust boundaries; use `check` when you want a bool without an error value.
