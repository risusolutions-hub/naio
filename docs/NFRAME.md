# NFRAME — Columnar DataFrame for Niao

`nframe` is a **pandas / polars** subset: typed `Series`, aligned `DataFrame`,
groupby, hash joins, reshape, missing-data fills, rolling windows, and CSV/JSON IO.
Implemented in `crates/niao_frame` (std + `niao_num` + `niao_data` only).

Import:

```niao
import "nframe"
```

## Series & DataFrame

| Function | Description |
|----------|-------------|
| `nframe.series(data, name?, dtype?)` | Build a typed column (`i64`/`f64`/`bool`/`str`/`date`) |
| `nframe.dataframe({col: series_or_array, …})` | Aligned multi-column table |
| `nframe.shape(df)` | `[nrows, ncols]` |
| `nframe.columns(df)` | Column name list |
| `nframe.select(df, names)` / `drop` / `rename` | Column projection |
| `nframe.with_column(df, series)` | Add or replace a column |
| `nframe.head(df, n?)` / `tail` / `slice` | Row windows |
| `nframe.filter(df, mask)` | Boolean mask filter |
| `nframe.sort(df, keys, descending?)` | Stable multi-key sort |
| `nframe.sample(df, n, seed?)` | Sample without replacement |

String columns use Arrow-style **offsets + bytes** (not `Vec<String>`).
Numeric series interop with `nnum` via `to_nnum`.

## GroupBy & Join

| Function | Description |
|----------|-------------|
| `nframe.group_by(df, keys).agg({col: "sum"\|"mean"\|…})` | Hash grouping |
| `nframe.join(left, right, on, how?)` | `inner` / `left` / `right` / `outer` |

Aggregations: `sum`, `mean`, `min`, `max`, `count`, `std`, `var`, `median`,
`first`, `last`, `n_unique`. Sample std/var use `ddof=1` (pandas default).

## Reshape & missing

| Function | Description |
|----------|-------------|
| `nframe.concat(frames, axis?)` | Stack rows (`0`) or columns (`1`) |
| `nframe.melt(df, id_vars, value_vars?)` | Wide → long |
| `nframe.pivot(df, index, columns, values)` | Long → wide (mean for dupes) |
| `nframe.explode(df, col, sep?)` | Split string lists into rows |
| `nframe.is_null` / `drop_nulls` / `fill_null` | Null bitmap ops (`value`/`ffill`/`bfill`/`mean`) |

## Window & ML glue

| Function | Description |
|----------|-------------|
| `nframe.rolling(series, n).mean/sum/std` | Sliding window |
| `nframe.cumsum` / `shift` / `diff` / `rank` | Series transforms |
| `nframe.to_nnum(df, cols?)` | Feature matrix → `nnum` array |
| `nframe.get_dummies(df, col)` | One-hot columns |
| `nframe.train_test_split(df, test_size, seed?)` | Row shuffle split |

## IO

| Function | Description |
|----------|-------------|
| `nframe.read_csv(path, opts?)` / `write_csv` | Typed CSV with dtype inference |
| `nframe.read_json(path)` / `write_json` | Array-of-objects JSON |

## Error codes (4010–4019)

| Code | Meaning |
|------|---------|
| 4010 | arity |
| 4011 | general error |
| 4012 | type mismatch |
| 4013 | unknown / bad column |
| 4014 | length mismatch |
| 4015 | dtype / unsupported op |

## v1 limitations

- No multi-index, categoricals, or timezone-aware timestamps (dates are days since epoch)
- Pivot uses mean aggregation for duplicate cells
- Null join keys match each other (document if you need pandas-NaN non-matching)
- Runtime builtins wired by orchestrator (`niao_runtime` + catalog)
