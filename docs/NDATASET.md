# ndataset — Dataset Loading and Batching

`ndataset` loads tabular data from common formats, supports train/val/test splits,
shuffling, filtering, and streaming batch iteration — a lightweight subset of
[HuggingFace datasets](https://huggingface.co/docs/datasets) and PyTorch
`DataLoader`.

Import with:

```niao
import "ndataset"
// or
import "std/ndataset"
```

Datasets are opaque integer handles backed by columnar storage (`nframe`). Batch
loaders are separate handles with their own cursor.

---

## Options

### CSV / JSON loaders

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `header` | bool | `true` | First CSV row is column names |
| `delimiter` | string | `","` | CSV field separator (single char) |

### `ndataset.split(ds, train_ratio, opts?)`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `val` | number | — | Validation fraction (optional) |
| `test` | number | — | Test fraction (optional; remainder if omitted) |
| `seed` | int | `0` | Shuffle seed before splitting |

### `ndataset.batch(ds, batch_size, opts?)`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `shuffle` | bool | `false` | Shuffle row order for this loader |
| `drop_last` | bool | `false` | Drop final partial batch |
| `seed` | int | `0` | Shuffle seed when `shuffle: true` |

---

## API

### `ndataset.from_rows(rows) -> handle`

Build a dataset from an array of objects. Column types are inferred
(bool → int → float → string).

```niao
let ds = ndataset.from_rows([
    {id: 1, label: "cat", score: 0.9},
    {id: 2, label: "dog", score: 0.4},
])
ndataset.len(ds)   // 2
```

---

### `ndataset.from_csv(path, opts?) -> handle`

Load a CSV file (header row by default).

```niao
let ds = ndataset.from_csv("data/train.csv", {header: true, delimiter: ","})
```

I/O and parse failures return catchable `ndataset_error` (code 4121).

---

### `ndataset.from_json(path) -> handle`

Load a JSON **array of objects**.

```niao
let ds = ndataset.from_json("data/records.json")
```

---

### `ndataset.from_jsonl(path) -> handle`

Load newline-delimited JSON (one object per line). Blank lines and `#` comments
are skipped.

```niao
let ds = ndataset.from_jsonl("data/stream.jsonl")
```

---

### `ndataset.len(handle) -> int`

Row count.

---

### `ndataset.columns(handle) -> string[]`

Column names in storage order.

---

### `ndataset.get(handle, index) -> object`

Single row by index (`-1` = last row). Out-of-range returns `ndataset_error`
(code 4125).

---

### `ndataset.select(handle, columns) -> handle`

New dataset with a column subset.

---

### `ndataset.filter_eq(handle, column, value) -> handle`

Keep rows where `column` equals `value`.

```niao
let cats = ndataset.filter_eq(ds, "label", "cat")
```

---

### `ndataset.shuffle(handle, seed?) -> handle`

Shuffled copy (Fisher–Yates). Original handle is unchanged.

---

### `ndataset.split(handle, train_ratio, opts?) -> {train, val?, test?}`

Shuffle-split into named portions. Ratios must sum to ≤ 1; remaining rows go to
`test` when `test` is omitted.

```niao
let parts = ndataset.split(ds, 0.8, {val: 0.1, seed: 42})
// parts.train, parts.val, parts.test
```

---

### `ndataset.concat(handles) -> handle`

Vertical stack (same columns required).

---

### `ndataset.take(handle, n) -> handle` / `ndataset.skip(handle, n) -> handle`

First `n` rows or skip first `n` rows.

---

### `ndataset.to_rows(handle) -> object[]`

Export all rows as an array of objects.

---

### `ndataset.batch(handle, batch_size, opts?) -> loader`

Create a streaming batch iterator over the dataset.

```niao
let loader = ndataset.batch(ds, 32, {shuffle: true, seed: 7})
while true {
    let batch = ndataset.next(loader)
    if batch == nil { break }
    // batch is object[] — one mini-batch
}
ndataset.reset(loader)   // rewind
```

Returns `nil` from `next` when exhausted.

---

### `ndataset.close(handle) -> bool` / `ndataset.close_loader(loader) -> bool`

Release handles. Using a closed handle returns `ndataset_error` (code 4123).

---

## Error codes

| Code | Kind | Meaning |
|------|------|---------|
| 4120 | arity | Wrong argument count |
| 4121 | error | I/O, parse, or operation failure |
| 4122 | type | Argument type mismatch |
| 4123 | invalid handle | Closed or unknown handle |
| 4124 | column | Unknown or invalid column |
| 4125 | index | Row index out of range |

All errors are catchable `ndataset_error` values.

---

## See also

- `nframe` — full DataFrame analytics (groupby, join, reshape)
- `ncsv` — lightweight CSV parse/stringify without dataset handles
- `nbatch` — adaptive batch sizing from memory budgets
