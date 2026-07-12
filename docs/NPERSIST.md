# npersist — Persistent Vector and HashMap

`npersist` wraps `im-rc` persistent collections with structural sharing. Mutating operations return **new handles**; older handles remain valid snapshots.

Import with:

```niao
import "npersist"
// or
import "std/npersist"
```

---

## Quick start

```niao
import "npersist"

let v0 = npersist.vec_new([1, 2])
let v1 = npersist.vec_push(v0, 3)   // v0 still len 2
print(npersist.vec_len(v0))          // 2
print(npersist.vec_len(v1))          // 3

let m0 = npersist.map_new({a: 1})
let m1 = npersist.map_set(m0, "b", 2)
print(npersist.map_get(m1, "b"))     // 2
print(npersist.share(v0, v1))        // false

npersist.close(v0)
npersist.close(v1)
npersist.close(m0)
npersist.close(m1)
```

---

## Vector API

| Method | Description |
|--------|-------------|
| `npersist.vec_new(items?)` | New persistent vector (optional array). |
| `npersist.vec_push(handle, value)` | Returns new handle with value appended. |
| `npersist.vec_set(handle, index, value)` | Returns new handle with updated index. |
| `npersist.vec_get(handle, index)` | Element at index. |
| `npersist.vec_len(handle)` | Length. |

## Map API

| Method | Description |
|--------|-------------|
| `npersist.map_new(obj?)` | New persistent map (optional object). |
| `npersist.map_set(handle, key, value)` | Returns new handle with key set. |
| `npersist.map_get(handle, key)` | Value or `nil`. |
| `npersist.map_keys(handle)` | Sorted key array. |
| `npersist.map_len(handle)` | Entry count. |

## Shared

| Method | Description |
|--------|-------------|
| `npersist.share(a, b)` | `true` when both handles reference the same persistent root (pointer equality). |
| `npersist.kind(handle)` | `"vector"` or `"map"`. |
| `npersist.close(handle)` | Free handle metadata. Returns `true` if it existed. |

---

## Errors

| Code | Meaning |
|------|---------|
| 3400 | Wrong argument count. |
| 3401 | Index/range/kind mismatch — catchable `npersist_error`. |
| 3402 | Wrong argument type. |
| 3403 | Invalid or closed handle — catchable `npersist_error`. |
