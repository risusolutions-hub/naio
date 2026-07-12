# nsketch — Probabilistic Sketches

`nsketch` provides compact probabilistic data structures for membership,
cardinality, and frequency estimation. Pure native Rust — no external crates;
hashing uses hand-rolled FNV-1a and xorshift.

Import with:

```niao
import "nsketch"
// or
import "std/nsketch"
```

Handles are opaque integers. Always call `nsketch.close(h)` when finished.

---

## Bloom filter

Approximate set membership. False positives are possible; false negatives are not.

### `nsketch.bloom_new(expected_n, fp_rate?) -> handle`

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `expected_n` | int | required | Expected number of insertions (must be > 0) |
| `fp_rate` | number | `0.01` | Target false-positive rate in `(0, 1)` |

```niao
let bf = nsketch.bloom_new(1000)
let bf = nsketch.bloom_new(1000, 0.001)
```

### `nsketch.bloom_add(h, string) -> true`

Insert a string into the filter.

### `nsketch.bloom_may_contain(h, string) -> bool`

Returns `true` if the string **may** be in the set (or is a false positive),
`false` if it is definitely absent.

### `nsketch.bloom_clear(h) -> true`

Reset all bits.

---

## HyperLogLog-lite

Cardinality estimate with a fixed **64 registers** (≈ few hundred bytes).

### `nsketch.hll_new() -> handle`

### `nsketch.hll_add(h, string) -> true`

Observe a string (duplicates are fine; they do not inflate the estimate much).

### `nsketch.hll_count(h) -> number`

Approximate distinct count (int or float). Accuracy is coarse with 64 registers —
useful for order-of-magnitude / growth checks, not exact analytics.

```niao
let h = nsketch.hll_new()
nsketch.hll_add(h, "a")
nsketch.hll_add(h, "b")
print(nsketch.hll_count(h))
```

---

## Count-Min Sketch

Approximate frequency of keys with overestimate-only guarantees (estimate ≥ true count).

### `nsketch.cms_new(width, depth) -> handle`

| Argument | Type | Description |
|----------|------|-------------|
| `width` | int | Columns per row (must be > 0) |
| `depth` | int | Number of hash rows (must be > 0) |

Larger `width`/`depth` → better accuracy, more memory.

### `nsketch.cms_add(h, string, count?) -> true`

Increment the counter for `string` by `count` (default `1`).

### `nsketch.cms_estimate(h, string) -> int`

Return the estimated count (≥ true frequency).

```niao
let cms = nsketch.cms_new(2048, 5)
nsketch.cms_add(cms, "user:42", 10)
print(nsketch.cms_estimate(cms, "user:42"))  // >= 10
```

---

## Common

### `nsketch.close(h) -> true | error`

Release a handle. Further use returns a catchable error.

### `nsketch.kind(h) -> "bloom" | "hll" | "cms" | error`

Report the sketch type for a live handle.

---

## Errors

| Code | Kind | Meaning |
|------|------|---------|
| 3000 | arity | Wrong argument count (thrown) |
| 3001 | `nsketch_error` | Semantic error (bad params, wrong kind) — catchable |
| 3002 | type | Wrong argument type (thrown) |
| 3003 | `nsketch_error` | Invalid or closed handle — catchable |

Wrong-kind calls (e.g. `bloom_add` on an HLL handle) return a catchable `nsketch_error`.

---

## Example

See `examples/nsketch_demo.niao`.
