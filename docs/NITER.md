# niter — iterator & combinatorics toolkit

Eager iterator utilities over general Niao values: Cartesian products, permutations, combinations, consecutive `groupby`, sliding `windows`, `chunked` batches, `flatten`/`chain`, `zip_longest`, and related helpers. Complements `nlazy` (packed numeric pipelines) with value-level combinators (~`itertools` / `more-itertools` subset).

## Import

```niao
import "niter"
```

Paths `import "std/niter"` and `import "niter"` are equivalent. Flat builtins (`niter_product`, `niter_combinations`, …) are also available globally after import.

## Quick start

```niao
import "niter"

// Cartesian product
print(niter.product([1, 2], ["a", "b"]))
// [[1, "a"], [1, "b"], [2, "a"], [2, "b"]]

// Combinations & permutations
print(niter.combinations([1, 2, 3, 4], 2))
print(niter.permutations("abc", 2))

// Sliding windows and chunks
print(niter.windows([1, 2, 3, 4, 5], 3))
print(niter.chunked([1, 2, 3, 4, 5], 2))

// Consecutive groupby (like itertools — not SQL GROUP BY)
let g = niter.groupby([1, 1, 2, 2, 1])
for grp in g { print(grp.key, grp.items) }
```

## Functions

| Method | Description |
|--------|-------------|
| `niter.product(a, b, …)` | Cartesian product of 1–16 arrays. Returns array of tuple-arrays. Empty pool → `[]`. |
| `niter.product_repeat(arr, n)` | Self-product `arr × … × arr` repeated `n` times. |
| `niter.combinations(arr, r)` | `r`-combinations without replacement. `r > len` → `[]`. |
| `niter.combinations_with_replacement(arr, r)` | Multisets of size `r`. |
| `niter.permutations(arr, r?)` | Permutations of length `r` (default: full length). |
| `niter.groupby(items)` | Consecutive runs of deep-equal values → `[{key, items}, …]`. |
| `niter.groupby_key(items, field)` | Consecutive runs with equal object field → `[{key, items}, …]`. |
| `niter.windows(items, size, step?)` | Full sliding windows (`step` default `1`). |
| `niter.chunked(items, size)` | Fixed-size chunks; last chunk may be shorter. |
| `niter.flatten(nested)` | One-level flatten of array-of-arrays. |
| `niter.chain(a, b, …)` | Concatenate 1–16 arrays. |
| `niter.zip_longest(a, b, fill?)` | Pairwise zip; missing slots use `fill` (default `nil`). |
| `niter.pairwise(items)` | Adjacent pairs `[[a,b], [b,c], …]`. |
| `niter.enumerate(items, start?)` | `[[index, value], …]` (`start` default `0`). |
| `niter.take(items, n)` | First `n` elements. |
| `niter.drop(items, n)` | Skip first `n` elements. |
| `niter.islice(items, start, stop?, step?)` | Slice with optional stop/step (like `range` indexing). |
| `niter.repeat(value, times)` | Repeat a value `times` times. |
| `niter.count(stop)` / `count(start, stop)` / `count(start, stop, step)` | Finite integer range as array. |
| `niter.unique_justseen(items)` | Drop consecutive duplicates (deep equality). |
| `niter.compress(data, selectors)` | Keep `data[i]` when `selectors[i]` is `true`. |

### Input types

All list-taking functions accept `Array` or packed arrays (`IntArray`, `FloatArray`, `BoolArray`, `StringArray`, `ByteArray`).

### Output limits

Combinatorial results are capped at **16,777,216** elements. Larger requests return a catchable `niter_error`.

## Errors

| Code | Meaning |
|------|---------|
| 3440 | Wrong argument count. |
| 3441 | Invalid parameter or output too large (catchable `niter_error`). |
| 3442 | Wrong argument type (hard error). |

## See also

- `nlazy` — fused lazy map/filter/take on packed numeric arrays.
- `npipe` — value op pipelines with built-in named stages.
- `dsa` — list/map/set data structures and sort/search helpers.
