# ndiff standard library

Deep structural equality and diff for Niao values: scalars, nested arrays/objects, and packed int/float arrays. Recursive paths use dotted keys and `[index]` segments (for example `a.b[0]`).

## Import

```niao
import "ndiff"
```

Paths `import "std/ndiff"` and `import "ndiff"` are equivalent. Flat builtins (`ndiff_equal`, `ndiff_diff`, `ndiff_summary`) are also available globally after import.

## Quick start

```niao
import "ndiff"

let a = {name: "vivek", tags: ["a", "b"], score: 10}
let b = {name: "vivek", tags: ["a", "c"], score: 12}

print(ndiff.equal(a, b))          // false

let d = ndiff.diff(a, b)
print(d.equal)                    // false
for c in d.changes {
    print(c.path, c.left, "→", c.right)
}
// tags[1] a → c
// score 10 → 12

print(ndiff.summary(d))
```

## Supported values

| Kind | Behavior |
|------|----------|
| `int`, `float`, `string`, `bool`, `nil` | Leaf compare via runtime `values_equal`. |
| `array` | Length + recursive element compare; paths `…[i]`. |
| `object` | Key union; missing side reported as `nil`; paths `….key`. |
| packed `IntArray` / `FloatArray` | Length + element compare (floats use epsilon). |

Unequal types at a path become a single change with the full left/right values.

## Functions

| Method | Description |
|--------|-------------|
| `ndiff.equal(a, b)` | Deep equality → `bool`. |
| `ndiff.diff(a, b)` | `{equal: bool, changes: [{path, left, right}, …]}`. |
| `ndiff.summary(diff_obj)` | Human-readable string (`"equal"` when identical; otherwise lines of `path: left → right`). |

## Errors

| Code | Meaning |
|------|---------|
| 3060 | Wrong argument count. |
| 3061 | Malformed diff object passed to `summary` (catchable). |
| 3062 | Type error (e.g. `summary` on a non-object). |
