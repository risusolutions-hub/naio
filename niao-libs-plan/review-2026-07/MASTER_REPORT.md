# Niao Standard Library — Full Review & v0.2.4 Plan

**Date:** 2026-07-13 · **Scope:** all 112 lib packages in `niao_libs/` · **Reviewer pass:** static source review + defect repair + roadmap

> This folder is a **staging area**. Nothing in your live tree was modified. Proposed manifest
> fixes, docs, and roadmaps live here for you to review and merge (`manifest-fixes/`,
> `docs-proposed/`, `harness/`). See `README.md` for how to apply.

---

## 1. What was done (and the one thing that wasn't)

- **Read the code of all 112 libs** at the architecture + full-API-surface level, with deep line-level
  reads of the performance-critical flagships (`nvec`, `nsimd`, `nrand`, `nmath`, `ncl`, `json`).
- **Audited every manifest** — found and repaired 35 corrupted/garbled JSON files.
- **Harvested the real benchmark numbers** already recorded in `benchmarks/` (v0.2.2 baseline).
- **Wrote a per-lib v0.2.4 roadmap** (`ROADMAP_v0.2.4.md`) — features, changes, and speed wins.
- **Wrote a deep performance analysis** (`PERFORMANCE.md`) with concrete, code-grounded optimizations.

**The honest gap:** I could not *execute* Niao or run the benchmarks in this pass. The built
`target/release/niao.exe` is a **Windows PE binary** and this review ran in a Linux sandbox with
**no Rust toolchain and no wine** — so I cannot compile or run `.niao` here. Performance claims below
are therefore either (a) **measured numbers already in your repo**, or (b) **static analysis** of the
actual source (specific lines, specific hot loops). To turn the static wins into measured numbers,
run the harness in `harness/` on your Windows box — it is written and ready.

---

## 2. Headline numbers

| Metric | Value |
|---|---|
| Lib packages | **112** |
| Runtime/crate source reviewed | **~93,300 LOC** |
| Registered builtins (counted) | **~1,495** |
| Libs with a dedicated doc | 97 / 112 |
| Manifests repaired this pass | **35** |
| Baseline arithmetic throughput (v0.2.2) | **345M ops/s** (14.3 MB RSS) — beats Node, 18.7× Python |

### Measured baseline (from `benchmarks/bench_full_results_0.2.2.txt`, Windows, release)

| Workload | Niao 0.2.2 | Node 24 | Python 3.11 |
|---|---|---|---|
| 10M mod-arithmetic (best) | **115.8 ms** | 120.1 ms | 2,162.9 ms |
| Peak RSS | **14.3 MB** | 37.5 MB | 20.6 MB |
| Throughput | **345M ops/s** | 333M ops/s | 18M ops/s |
| `fib(40)` | **0.08 ms** | — | — |
| Test suite | 31 passed, 0 failed | | |

The VM is already fast and lean. The wins below are about the **library layer** (numeric kernels,
allocation discipline, and feature completeness), not the interpreter core.

---

## 3. Architecture in one paragraph (so the recommendations make sense)

Every Niao value is a `ValueRef = Rc<RefCell<Value>>` — reference-counted with interior mutability.
That gives cheap sharing and a simple GC story, but it means **per-value refcount + borrow-flag
overhead** on any hot loop that touches values individually. The runtime already dodges this for
numerics with **packed `FloatArray` / `IntArray`** columns (contiguous `Vec<f64>`/`Vec<i64>`), which
is why `ncl`, `nml`, `nsimd`, `npar`, and `nlazy` operate on packed buffers instead of arrays of
`ValueRef`. **The single biggest lever for "more speed" is widening packed-array coverage and making
the packed kernels true-SIMD.** Details in `PERFORMANCE.md`.

---

## 4. Cross-cutting findings (apply to many libs at once)

| # | Finding | Evidence | Impact |
|---|---|---|---|
| C1 | **Numeric kernels are autovectorized-scalar, not true SIMD.** `nsimd` uses `chunks_exact(8)` unrolled loops and relies on the compiler; `nvec::cosine` is a plain scalar `f32` loop. | `nsimd.rs` header comment; `nvec/index.rs:84` | 2–4× on numeric hot paths |
| C2 | **`nvec` recomputes vector norms on every search.** `cosine()` recomputes the query norm N times and stored-vector norms every call instead of precomputing. | `nvec/index.rs:84-95` | ~2–3× on vector search |
| C3 | **Low `#[inline]` density on hot kernels.** Only ~70 `#[inline]` in the whole runtime; numeric helper fns crossing module boundaries won't inline. | grep: 70 hits | 5–15% on kernels |
| C4 | **Allocation churn: push-loops without `with_capacity`.** `json` (43 pushes), `ntoml` (27), `ncanon` (25), `dsa` (25), `nurl` (22) grow `Vec`/`String` without reserving. (~220 `with_capacity` calls exist elsewhere — the discipline is there, just uneven.) | grep push-sites | 10–30% on parse/serialize |
| C5 | **`.clone()`-heavy request paths.** `nmem` (45), `nvec` (44), `nmongo` (65 across files), `ndebug` (27), `nargs` (25), `nagent` (25) clone values where borrows would do. | grep `.clone()` | alloc + latency |
| C6 | **Only 8 `unsafe` blocks total.** Very safe — good — but the SIMD/packed kernels leave `get_unchecked` / aligned-load speed on the table where bounds are provably safe. | grep: 8 hits | kernel-local |
| C7 | **Packed-array generation gap.** `nrand` core is already optimal (Lemire bounded ints + Box-Muller spare cache — verified in source); the remaining lever is a `fill(FloatArray)` path so bulk generation skips per-element `ValueRef` alloc. Same pattern applies wherever a lib builds a numeric array element-by-element. | `nrand.rs:83,107` | removes N allocs on bulk gen |
| C8 | **Encoding hygiene.** UTF-8 BOM in several `.rs` sources and manifests; NUL bytes corrupting 12 `package.json`; double-encoded em-dashes in 9 manifests. | see §5 | correctness/tooling |

---

## 5. Defects found and repaired (staged in `manifest-fixes/`)

All 35 corrected files are written to `manifest-fixes/` — none applied to your live tree yet.

| Class | Count | Files | Fix |
|---|---|---|---|
| **NUL-corrupted `package.json`** (JSON parse fails: "Extra data") | 12 | core, dsa, io, json, nenv, net, nmongo, nos, npg, nsqlite, parallel, re, time | Strip trailing `\x00` bytes; re-serialize clean UTF-8 (LF) |
| **BOM + bad `0.2.2/lib.json`** | 12 | same set | Strip `﻿` BOM; re-serialize |
| **Truncated manifests** | 2 | `io/package.json`, `io/0.2.2/lib.json` (cut mid-`builtin_count`) | Reconstructed from visible fields; **`builtin_count` flagged `_FIXME`** — fill from runtime |
| **Mojibake descriptions** (`â€"` → `—`) | 9 | ncl, nml, nvis (package + both lib.json) | Re-decode cp1252→utf-8 |

> **Action on `io`:** the original manifest was physically truncated, so the true `builtin_count`
> is unknown. I set it to `0` with an `_FIXME` note. Grep the runtime's io registration to fill it
> before merging.

### Missing docs (15 libs, `docs/` has no matching file)

These split into two groups:

- **Superseded aliases (5)** — older crate-backed names with a newer native `n`-prefixed twin that
  *is* documented. Recommend a one-line "moved to X" stub, not a full doc:
  `args`→`nargs`, `log`→`nlog`, `rand`→`nrand`, `codec`→(nfmt/ncanon), `collections`→(dsa maps/sets).
- **Genuinely undocumented (10)** — deserve real docs: `archive`, `bignum`, `crypto`, `http`, `io`,
  `net_clients`, `nos`, `nllm`, `nrag`, `core`.

Proposed docs for the highest-value gaps are in `docs-proposed/`.

---

## 6. Consolidation opportunity (lightweight win)

Eleven un-prefixed, crate-backed libs predate the native `n*` stdlib and overlap with it:
`archive, args, bignum, codec, collections, crypto, http, io, log, net_clients, rand`.
Several have direct native twins (`args`↔`nargs`, `log`↔`nlog`, `rand`↔`nrand`). Carrying both
inflates the catalog, the binary, and the docs surface. **Recommendation:** pick one canonical name
per capability, alias the other, and delete the dead code path once callers migrate. This is the
cleanest "lightweight" win — it removes whole modules rather than shaving allocations.

---

## 7. Top 10 highest-impact changes for v0.2.4

Ranked by (impact × how many users touch it) ÷ risk. Full per-lib detail in `ROADMAP_v0.2.4.md`.

1. **`nvec`: precompute norms + SIMD cosine** — store `1/‖v‖` at insert, dot-only at query, 8-wide `f32`. ~2–3×. (C1, C2)
2. **`nsimd`: port kernels to `std::simd`** with scalar fallback — real vectorization for `ncl`/`nml`/`npar`. 2–4×.
3. **`ncl`: capacity-hint + fused kernels** on groupby/join/agg; avoid intermediate `Vec` per column. 10–30%.
4. **`json`: reserve output buffer + SIMD whitespace/string scan** on parse. 15–40% on big docs.
5. **`nrand`: add `fill(FloatArray)` packed generation + new distributions** (poisson/binomial/gamma, `uuid4`). Core PRNG is already optimal — this is packed-alloc removal + features, not a scalar fix.
6. **`nstr`: add `format`/`sprintf`, `find_all`, Unicode NFC/NFD**, and grapheme-aware ops. Feature gap.
7. **`nmath`: add `erf`/`gamma`/`fma`/`cumsum`/`corr`/`histogram`/`quantile`** — common stats holes.
8. **`ncache`: LFU + single-flight (stampede protection) + batch `get_many`.** Feature + latency.
9. **Repair 35 manifests + fill `io` count** (staged) — unblocks tooling that parses the catalog.
10. **Consolidate the 11 legacy crate-libs** into their native twins — smaller binary, cleaner docs.

---

## 8. How performance was assessed (transparency)

- **Measured:** numbers in §2 come straight from `benchmarks/bench_full_results_0.2.2.txt` and
  `bench_results_0.2.2.txt` — real runs on your machine at v0.2.2.
- **Static:** every "N×" estimate below a specific line reference is an *engineering estimate* from
  reading the loop (FLOP count, allocation count, vectorization width), **not a measured result**.
  They are labeled as estimates in `PERFORMANCE.md`.
- **To measure:** `harness/run_bench.py` + `harness/README.md` regenerate the full comparison table
  and add per-lib micro-benchmarks. Run on Windows where `niao.exe` lives.

See `ROADMAP_v0.2.4.md` for the per-library breakdown and `PERFORMANCE.md` for the optimization detail.
