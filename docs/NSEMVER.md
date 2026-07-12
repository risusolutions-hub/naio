# nsemver standard library

SemVer 2.0 parsing, comparison, range matching, and version increment. Hand-rolled parser — no external crate.

## Import

```niao
import "nsemver"
```

Paths `import "std/nsemver"` and `import "nsemver"` are equivalent. Flat builtins (`nsemver_parse`, `nsemver_compare`, …) are also available globally after import.

## Quick start

```niao
import "nsemver"

let v = nsemver.parse("1.2.3-alpha.1+build.42")
print(v.major, v.minor, v.patch)   // 1 2 3
print(v.pre)                       // alpha.1
print(v.build)                     // build.42

print(nsemver.compare("1.2.3", "1.2.4"))   // -1
print(nsemver.valid("not-a-version"))      // false
print(nsemver.satisfies("1.5.0", "^1.2.3")) // true
print(nsemver.inc("1.2.3", "minor"))       // 1.3.0
```

## Functions

| Method | Description |
|--------|-------------|
| `nsemver.parse(version)` | Returns `{major, minor, patch, pre, build}` on success, catchable `error` on invalid input. `pre` and `build` are empty strings when absent. |
| `nsemver.compare(a, b)` | Returns `-1`, `0`, or `1` (like strcmp). Catchable `error` if either operand is invalid. |
| `nsemver.valid(version)` | `true` when `version` is valid SemVer 2.0. |
| `nsemver.satisfies(version, range)` | `true` when `version` matches every space-separated comparator in `range`. Invalid `version` → `false`; invalid `range` → catchable `error`. |
| `nsemver.inc(version, part)` | Bump `major`, `minor`, or `patch`; clears pre-release and build metadata. Catchable `error` on invalid version or part. |

## Range syntax

Space-separated comparators are combined with **AND** (all must match).

| Form | Meaning |
|------|---------|
| `1.2.3` | Exact match (`=1.2.3`). |
| `^1.2.3` | `>=1.2.3 <2.0.0` (caret rules for `0.x` follow npm: `^0.2.3` → `<0.3.0`, `^0.0.3` → `<0.0.4`). |
| `~1.2.3` | `>=1.2.3 <1.3.0` (`~1` → `>=1.0.0 <2.0.0`, `~1.2` → `>=1.2.0 <1.3.0`). |
| `>=1.0.0` / `<=2.0.0` / `>1.0.0` / `<2.0.0` / `=1.0.0` | Standard inequality / equality. |
| `^1.2.3 >=2.0.0` | Multiple comparators (AND). |

Pre-release ordering follows SemVer 2.0: `1.0.0-alpha` &lt; `1.0.0`. Build metadata is ignored for ordering and range checks.

## Errors

| Code | Meaning |
|------|---------|
| 2900 | Wrong argument count. |
| 2901 | Invalid increment part or other semantic error (catchable). |
| 2902 | Parse error — invalid version or range (catchable). |
