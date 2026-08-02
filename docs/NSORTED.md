# nsorted standard library

Sorted list / dict / set with bisect insert, range queries, and nearest lookup. Native Rust implementation (~sortedcontainers + bisect subset).

## Import

```niao
import "nsorted"
```

Paths `import "std/nsorted"` and `import "nsorted"` are equivalent. Flat builtins (`nsorted_new_list`, `nsorted_bisect_left`, …) are also available globally after import.

## Quick start

```niao
import "nsorted"

let sl = nsorted.new_list([3, 1, 2, 2])
nsorted.add(sl, 2)
print(nsorted.bisect_left(sl, 2))     // 1
print(nsorted.irange(sl, 2, 4))        // [2, 2, 3]

let sd = nsorted.new_dict({c: 3, a: 1, b: 2})
print(nsorted.keys(sd))                // ["a", "b", "c"]
print(nsorted.peekitem(sd))            // {key: "a", value: 1}

let ss = nsorted.new_set([5, 1, 3])
print(nsorted.nearest(ss, 4))           // 3 or 5

// Standalone bisect on a sorted int array
print(nsorted.bisect_right_arr([1, 3, 3, 5], 3))  // 3

nsorted.close(sl)
nsorted.close(sd)
nsorted.close(ss)
```

## Constructors

| Method | Description |
|--------|-------------|
| `nsorted.new_list(items?)` | Sorted multiset (duplicates allowed). Optional array. |
| `nsorted.new_set(items?)` | Sorted unique set. Optional array. |
| `nsorted.new_dict(obj?)` | Sorted dict (keys ordered). Optional object. |
| `nsorted.close(handle)` | Free handle. Returns `true` if it existed. |
| `nsorted.kind(handle)` | `"list"`, `"set"`, or `"dict"`. |

## Mutations

| Method | Description |
|--------|-------------|
| `nsorted.add(h, value)` | Insert into list/set. Returns `true` for set when new. |
| `nsorted.add_many(h, items)` | Bulk insert; returns count. |
| `nsorted.set(h, key, value)` | Dict only. Returns previous value or `nil`. |
| `nsorted.discard(h, value)` | Remove one occurrence (list) or member (set/dict key). |
| `nsorted.remove(h, value)` | Like discard but errors if missing. |
| `nsorted.pop(h, index?)` | Remove by sorted index (default: last). |
| `nsorted.clear(h)` | Drop all entries. |

## Lookups

| Method | Description |
|--------|-------------|
| `nsorted.len(h)` | Element / entry count. |
| `nsorted.get(h, index_or_key)` | List/set: index. Dict: key → value or `nil`. |
| `nsorted.contains(h, value)` | Membership test. |
| `nsorted.count(h, value)` | List only — duplicate count. |
| `nsorted.index(h, value)` | First sorted index of value. |
| `nsorted.min(h)` / `nsorted.max(h)` | Smallest / largest key (dict: key only). |

## Bisect & range

| Method | Description |
|--------|-------------|
| `nsorted.bisect_left(h, value)` | Insertion index (left of equal run). |
| `nsorted.bisect_right(h, value)` | Insertion index (right of equal run). |
| `nsorted.insort(h, value, side?)` | Insert at bisect position (`"left"` / `"right"`). |
| `nsorted.irange(h, min, max, opts?)` | Values (or dict pairs) in key range. `opts`: `{min_inclusive, max_inclusive}` (default both `true`). |
| `nsorted.islice(h, start, stop?)` | Index slice as array. |
| `nsorted.nearest(h, value, side?)` | Closest value: `"left"`, `"right"`, or `"nearest"` (default). |

## Dict views

| Method | Description |
|--------|-------------|
| `nsorted.keys(h)` | Sorted keys (list/set: all values). |
| `nsorted.values(h)` | Dict values in key order. |
| `nsorted.items(h)` | Array of `{key, value}` objects. |
| `nsorted.peekitem(h, index?)` | Dict item at sorted index (`0` = min, `-1` = max). |
| `nsorted.to_array(h)` | Materialize list/set values or dict items. |

## Array bisect (no handle)

| Method | Description |
|--------|-------------|
| `nsorted.bisect_left_arr(arr, value)` | `bisect_left` on sorted int array. |
| `nsorted.bisect_right_arr(arr, value)` | `bisect_right` on sorted int array. |
| `nsorted.insort_arr(arr, value, side?)` | Returns new sorted int array with value inserted. |

Int-only fast path: homogeneous int lists use a packed `Vec<i64>` with binary-search insert. Mixed types promote to a B-tree multiset automatically.

## Errors

| Code | Meaning |
|------|---------|
| 3450 | Wrong argument count. |
| 3451 | Operation failed (not found, empty, type mismatch) — catchable `nsorted_error`. |
| 3452 | Wrong argument type. |
| 3453 | Invalid or closed handle — catchable `nsorted_error`. |

## Deferred vs sortedcontainers

Not in v0.1.0: `SortedKeyList`, weighted samples, disk-backed stores, custom key functions, and parallel bulk merge (use `npar` on exported arrays instead).
