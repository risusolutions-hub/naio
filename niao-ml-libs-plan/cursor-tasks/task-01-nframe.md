# Task 01 — nframe: pandas / polars DataFrame (crate `niao_frame`)
Wave 1 (needs nnum). Read `../MASTER_PLAN.md` + `../specs/niao_frame__nframe.md`. Error block **4010–4019**.
Depends on: `nnum`, `niao_data`, `ncsv`, `njson`.

## Build (`crates/niao_frame`, zero new deps — reuse niao_data + ncsv/njson)
- Columnar `Series` (i64/f64/bool/str/date) with a null bitmap; string columns as offset+bytes (Arrow-style), NOT
  `Vec<String>`. Numeric series interop zero-copy with nnum.
- `DataFrame`: select/drop/rename/with_column/filter(mask+predicate)/sort(multi-key stable)/slice/sample/head/tail.
- GroupBy: hash grouping → agg(sum/mean/min/max/count/std/var/median/first/last/n_unique).
- Join: hash join inner/left/right/outer (incl null + many-to-many). Reshape: pivot/melt/concat/explode.
- Missing: is_null/drop_nulls/fill_null(value/ffill/bfill/mean). Window: rolling(n).mean/sum/std, cumsum, shift, diff, rank.
- IO: read_csv/write_csv(ncsv), read_json/write_json(njson), dtype inference. ML glue: to_nnum, get_dummies,
  train_test_split(→ntune).

## Wire up
- `niao_libs/nframe/` wrapper + builtins; `docs/NFRAME.md`; `examples/nframe_demo.niao` (load→groupby→join→to_nnum).

## Acceptance
- read/write csv round-trip; groupby aggs vs pandas rtol 1e-10; join correctness vs pandas (all 4 hows + null keys);
  fill_null/rolling/get_dummies vs pandas fixtures.
- unknown col→4013, length mismatch→4014, dtype error→4015.
- `benchmarks/benchmark_nframe.py` vs pandas; groupby+join within 3x on 1M rows. `cargo test -p niao_frame` green.

See `../cursor-rules.md`.
