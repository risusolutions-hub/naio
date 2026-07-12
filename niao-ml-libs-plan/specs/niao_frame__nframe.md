# Library spec: `nframe`  →  crate `niao_frame`

| | |
|---|---|
| Category | Data / DataFrame |
| Replaces (Python) | `pandas` / `polars` |
| Rust reference | `polars` (Arrow-style columnar layout) |
| Target Niao crate | `crates/niao_frame` |
| Niao import name | `nframe` |
| Difficulty | 4/5 — Very Hard |
| Wave | 1 (needs nnum) |
| Depends on Niao libs | `nnum`, `niao_data`, `ncsv`, `njson` |
| Error block | 4010–4019 |

## Goal
A columnar `DataFrame` + typed `Series` for loading, cleaning, joining, grouping, and reshaping tabular data —
the plumbing every classical-ML workflow needs before `nlearn`/`nboost`. **Zero external deps.** Reuse the
existing `niao_data` columnar primitives and `ncsv`/`njson` for IO; do not re-invent them.

## Scope (v1)
- **Series (typed column):** dtypes `i64`, `f64`, `bool`, `str`, `date` (via ntime); a validity/null bitmap.
  Numeric series interop with `nnum` arrays zero-copy where contiguous.
- **DataFrame:** ordered named columns, `head/tail`, `select`, `drop`, `rename`, `with_column`, `filter` (boolean mask
  + predicate expressions), `sort` (multi-key, stable), `slice`, `sample`.
- **GroupBy:** `group_by(keys).agg(...)` with `sum/mean/min/max/count/std/var/median/first/last/n_unique`;
  hash-based grouping over a columnar key encoding.
- **Join:** `join(other, on, how)` for `inner/left/right/outer`; hash join with a build/probe side choice.
- **Reshape:** `pivot`, `melt`, `concat` (axis 0/1), `explode`.
- **Missing data:** `is_null`, `drop_nulls`, `fill_null` (value / forward / backward / mean).
- **Rolling / window:** `rolling(n).mean/sum/std`, `cumsum/cumcount`, `shift`, `diff`, `rank`.
- **IO:** `read_csv`/`write_csv` (via ncsv), `read_json`/`write_json` (via njson); dtype inference + schema override.
- **ML glue:** `to_nnum()` (feature matrix), `get_dummies` (one-hot), `train_test_split` (delegate to `ntune`).

## Implementation blueprint (make it FAST + LIGHT)
- **Columnar, not row-wise.** Each column is one contiguous buffer + optional null bitmap (1 bit/row). This is
  what makes groupby/join/filter fast and cache-friendly (see polars/Arrow).
- GroupBy/join: encode keys → hash into an open-addressing table (reuse `niao_collections` if available); collect
  row indices per group; aggregate over contiguous slices. No per-row boxing.
- Filter: build a boolean mask column, then gather with a single strided pass; predicate expressions compile to
  a small closure tree, not per-cell dispatch.
- String columns: offset+bytes layout (Arrow-style `Vec<u8>` + `Vec<u32>` offsets), not `Vec<String>`.
- Sort: indirect argsort (indices), stable, multi-key radix/merge for numeric keys.

### Performance rules
- No per-row allocation; operate on whole columns. Pre-size output buffers from known lengths.
- `#[inline]` hot accessors; avoid `dyn` in inner loops (use an enum of column kinds, match once per column).
- Reuse `nnum` for numeric column math; reuse `ncsv`/`njson` for parsing.

## Public API surface
`DataFrame`, `Series`, `read_csv/write_csv`, `group_by().agg()`, `join`, `pivot/melt/concat`, `filter/sort/select`,
`fill_null/drop_nulls`, `rolling`, `to_nnum/get_dummies`. Expose to Niao via `niao_libs/nframe/` + builtins
(mirror `niao_libs/nvalid`). Niao surface is fluent: `nframe.read_csv(path).group_by("k").agg(...)`.

## Performance target
- `read_csv` of a 100 MB file, `group_by`, and `inner join` on 1M rows each within **3×** of pandas wall-clock.
- Filter + select within **2×** of pandas. Memory ≤ pandas for the same frame.

## Tests required
- Round-trip `read_csv`→`write_csv` cell-equivalent (allowing float formatting) on a fixture CSV.
- GroupBy aggregations vs pandas fixtures (sum/mean/std/median) on a seeded frame, `rtol=1e-10`.
- Join correctness vs pandas for inner/left/right/outer including null keys and many-to-many.
- Null handling: `fill_null(mean)`, `drop_nulls`, forward-fill vs pandas fixtures.
- Rolling mean/std vs pandas; `get_dummies` vs pandas one-hot.
- Degenerate: unknown column → 4013; length mismatch on `with_column` → 4014; dtype op error → 4015.
- Plus: in-crate unit tests, `examples/nframe_demo.niao`, `benchmarks/benchmark_nframe.py` vs pandas.

## Risk / notes
- Breadth is the cost — ship the columns above and the listed verbs, resist adding pandas' long tail (multi-index,
  categoricals, timezones beyond ntime) in v1; document as v2.
- Get the null bitmap right early; retrofitting nulls is painful.
- String-column layout choice is load-bearing for join/groupby speed — do the offset+bytes design, not `Vec<String>`.

## Done criteria
- `cargo check --workspace` and `cargo test -p niao_frame` green; fixtures match pandas within tolerance.
- `niao_libs/nframe/` wrapper + `examples/nframe_demo.niao` runs an end-to-end load→groupby→join→to_nnum.
- Benchmark logged in `REPORT.md`; `CHANGELOG.md` updated; shared-file edits reported, not applied.
