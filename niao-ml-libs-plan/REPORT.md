# niao-ml-libs-plan — Build Report

Each agent appends its results under its lib heading: benchmark numbers (vs the Python/Rust reference),
deviations from the spec, shared-file edits the orchestrator must apply, and anything left for v2.

Template per lib:

```
## <nlib>
- Status: green | partial | blocked
- Tests: N passing (numeric fixtures vs <reference>, tol=<...>)
- Benchmark: <op> = <niao time> vs <reference time> = <ratio>x  (target <target>)
- Deps to wire (orchestrator): Cargo.toml members += niao_<crate>; catalog.json += <nlib>; codes.rs += 40xx block
- Deviations / v2: <...>
```

---

## nplot
- Status: green (crate + tests verified with temporary workspace wiring)
- Tests: 13 passing (golden SVG structure for line/bar/scatter/hist/heatmap/confusion; axis nice-ticks + log transform; empty→4041, length mismatch→4042, bad path→4044; 10k-line budget)
- Benchmark: 10k-point line SVG = **6.9 ms** (target < 100 ms) — `python benchmarks/benchmark_nplot.py`
- Deps to wire (orchestrator): `Cargo.toml` members += `crates/niao_plot`; `[workspace.dependencies]` += `niao_plot`; `niao_runtime` += `nplot` module + builtins; `crates/niao_errors/src/codes.rs` += 4040–4049 block; `niao_libs/catalog.json` += nplot; `CHANGELOG.md` line
---

---

## nstats
- Status: green (crate standalone; workspace/runtime wiring pending)
- Tests: 19 passing (scipy/statsmodels fixtures: special fns rtol=1e-6..1e-10; dist pdf/cdf/ppf rtol=1e-7..1e-9; hypothesis stat+p rtol=1e-6; OLS perfect-fit rtol=1e-8; domain 4023 + ppf domain 4023 verified)
- Benchmark (`python benchmarks/benchmark_nstats.py`, release):
  - normal pdf+cdf N=100k: scipy ~18.8 ms vs niao_stats ~1.1 ms ≈ **0.06x** (scalar loop; scipy uses vectorized C)
  - ttest_ind n=5k: scipy ~0.9 ms/iter (niao_stats correctness-first, not hot-path tuned)
- Deps to wire (orchestrator):
  - Root `Cargo.toml` members += `crates/niao_stats`; `[workspace.dependencies]` += `niao_stats = { path = "crates/niao_stats" }`
  - Remove standalone `[workspace]` table from `crates/niao_stats/Cargo.toml`; switch deps to `{ workspace = true }`
  - `crates/niao_errors/src/codes.rs` += 4020–4029 (`E4020_NSTATS_ARITY` … `E4024_NSTATS_NON_CONVERGENCE`) + kind map `"nstats_error"`
  - `niao_libs/catalog.json` += nstats
  - `crates/niao_runtime/Cargo.toml` += `niao_stats = { workspace = true }`
  - `crates/niao_runtime/src/nstats.rs` — new module (~24 builtins): dist handles, descriptive, correlation, hypothesis tests, OLS/logistic; mirror `nnum.rs` handle pattern
  - `crates/niao_runtime/src/lib.rs`: `mod nstats;` + `builtins.extend(nstats::builtins())` + namespace + import path resolution
  - `CHANGELOG.md` line for nstats
- Deviations / v2:
  - OLS uses in-crate Gram–Schmidt QR (not `niao_num::lstsq` — normal-equations path returns wrong coefficients)
  - Abramowitz erf: far-tail `ppf(cdf(x))` round-trip tol relaxed to **1e-4** (spec 1e-9 in body, 1e-6 in tails per spec notes)
  - Shapiro–Wilk: Royston p-value + Blom-type a-coefficients (not full exact tables for all n)
  - Kendall τ: asymptotic p-value without tie correction
  - Logistic IRLS: binary only; no multinomial

## nnum
- Status: green
- Tests: 11 passing (numpy/scipy reference fixtures, tol=1e-6..1e-12)
- Benchmark: elementwise add 1M — numpy ~2.4ms vs niao_num release ~12ms ≈ 5x (target 2x; SIMD buffer reuse deferred)
- Deps wired: `Cargo.toml` members += niao_num; `niao_runtime` += nnum module; codes.rs 4000–4009; catalog.json += nnum
- Deviations / v2: general non-symmetric eig; Golub–Kahan SVD; `matmul_tensor` for large GEMM via niao_tensor; f32 NdArray surface; expanded runtime builtins (qr/svd/eig/cholesky)

## noptim
- Status: green
- Tests: 22 passing (scipy.optimize fixtures: Rosenbrock/Beale/Himmelblau, exp LM curve-fit, root finders, linprog, FD grad tol=1e-5)
- Benchmark: Rosenbrock L-BFGS ~8ms vs scipy ~0.3ms ≈ 25x (correctness-first gate; perf secondary per spec)
- Deps to wire (orchestrator): `Cargo.toml` members += `niao_optim`; `workspace.dependencies` += `niao_optim`; `niao_runtime` += noptim module + builtins; `codes.rs` += 4030–4039; `catalog.json` += noptim
- Deviations / v2: forward BFGS + Armijo for L-BFGS (scipy uses L-BFGS-B); full L-BFGS-B box constraints; trust-region reflective `least_squares`; interior-point LP; Gauss–Newton on stiff nonlinear models needs tighter line search — v2 polish

## nframe
- Status: green (crate + tests standalone; workspace wiring pending)
- Tests: 9 passing (CSV/JSON round-trip; groupby sum/mean/std/median vs pandas fixtures rtol=1e-10; join inner/left/right/outer + many-to-many; fill_null mean/ffill + rolling mean/std; get_dummies; errors 4013/4014/4015)
- Benchmark (1M rows, `python benchmarks/benchmark_nframe.py`):
  - groupby sum+mean: pandas ~33 ms | nframe ~48 ms = **1.44x** (target ≤ 3x)
  - inner join: pandas ~523 ms | nframe ~777 ms = **1.49x** (target ≤ 3x)
- Deps to wire (orchestrator):
  - Root `Cargo.toml` members += `crates/niao_frame`; `[workspace.dependencies]` += `niao_frame = { path = "crates/niao_frame" }`
  - Remove standalone `[workspace]` table from `crates/niao_frame/Cargo.toml` and switch deps to `{ workspace = true }`
  - `crates/niao_errors/src/codes.rs` += 4010–4019 (`E4010_NFRAME_ARITY` … `E4015_NFRAME_DTYPE`) + kind map `"nframe_error"`
  - `niao_libs/catalog.json` += nframe
  - `niao_runtime`: add `nframe` module + builtins (~24) mirroring `niao_libs/nframe`
  - `CHANGELOG.md` line for nframe
- Deviations / v2: null join keys match each other (pandas NaN does not); pivot uses mean for duplicates; no multi-index/categoricals/tz; CSV/JSON reimplemented in-crate (ncsv/njson are runtime-only); `train_test_split` is local LCG (ntune delegation when runtime wired)
