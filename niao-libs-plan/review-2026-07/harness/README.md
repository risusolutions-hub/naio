# Per-lib benchmark harness

Because the review ran in a Linux sandbox and your `niao` build is a Windows binary, **no benchmarks
were executed during the review** — the "N×" figures in `PERFORMANCE.md` are static estimates. This
harness turns them into real numbers. Run it on Windows.

## Run

```powershell
cd niao-libs-plan\review-2026-07\harness
python run_bench.py                 # all benches, 5 runs each
python run_bench.py nrand nvec      # only matching benches
python run_bench.py --runs 9        # more runs for tighter numbers
```

It finds `niao` the same way your existing `benchmarks/` scripts do (PATH, `~/.cargo/bin/niao.exe`,
`target/release/niao.exe`), warms the compile cache once, then times each snippet with the VM's own
`time` reporting. Results are written to `results/<UTC-timestamp>.json`.

## Measure an optimization

1. `python run_bench.py` on `main` → baseline JSON.
2. Land one change from `ROADMAP_v0.2.4.md` (e.g. nvec norm precompute).
3. `python run_bench.py` again → new JSON.
4. Diff the two `best_ms` values. That delta is your real speedup — put it in `REPORT.md`.

## The benches (`benches/*.niao`)

| Bench | Targets | PERFORMANCE.md |
|---|---|---|
| `aa_control_arith` | VM baseline (no stdlib) — read others relative to this | §0 |
| `nrand_ranged_int` | ranged-int throughput | §5 (Lemire) |
| `nrand_normal` | normal-dist throughput | §5 (Box-Muller cache) |
| `nmath_scalar` | transcendental hot loop | §6 |
| `nstr_transform` | alloc-heavy string transforms | §6/§7 (reserve) |
| `json_parse` | parse throughput | §4 (reserve + SIMD scan) |
| `ncache_churn` | LRU set/get churn | §E (O(1) LRU) |
| `nvec_search` | cosine search (biggest win) | §1 (norms + SIMD) |

## If a bench errors

The runner reports `ERROR` and keeps going. Snippets use idiomatic API guesses; a few (notably
`nvec_search`, and `ncache_churn`'s `str()` int→string call) are marked **verify** because their exact
builtin names weren't fully enumerated in the review. Open the snippet, fix the call to match your
build, and re-run. Add your own snippets freely — any `benches/*.niao` with a `main()` is picked up.
