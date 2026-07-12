# Niao Advanced Batch 3 — DX, AI-Native, Performance (30 New Libraries)

Source: the 42-item "NEKO — ADVANCED CONCEPTS" wishlist. This plan maps every item
to a library (new or existing) or an honest VM-roadmap note, allocates error codes,
and defines the parallel build protocol.

Also completed in this batch: **central integration of the previously-built but
unwired modules** (expansion batch 2, unique batch, AI/HW batch) — 37 modules that
existed on disk but were never wired into `lib.rs` / `codes.rs` / catalogs.

## 42-item disposition table

| # | Item | Disposition |
|---|------|-------------|
| 1 | nhotreload | **NEW `nhotreload`** — file watch + per-function body diff via parser (live VM swap = roadmap) |
| 2 | ntrace | **NEW `ntrace`** — spans, W3C traceparent, events, JSON export |
| 3 | nreplay | EXISTS — wired this batch |
| 4 | nmigrate-diff | **NEW `nmigrate`** — schema diff → SQL (sqlite/postgres); nmodel.migrate covers create-path |
| 5 | REPL-in-file | **NEW `nrepl`** — subprocess eval sessions (`--watch-expr` CLI flag = roadmap) |
| 6 | ndoc doctests | **NEW `ndoc`** — `// >>>` / `// =>` doc-comment tests, extracted + executed |
| 7 | nconfig | **NEW `nconfig`** — defaults → file (json/toml) → env → args, typed schema |
| 8 | ndebug | **NEW `ndebug`** — checkpoint time-travel over values + deep diff (opcode scrub = roadmap) |
| 9 | nbench | **NEW `nbench`** — `run(name, fn)` warmup + mean/p50/p95/p99, compare |
| 10 | nlint rule API | **NEW `nlint`** — AST-as-data via niao_parser + data-driven / custom-fn rules |
| 11 | nworkspace | **NEW `nworkspace`** — workspace manifest, member graph, topo order, run |
| 12 | nerrgen | **NEW `nerrgen`** — E-code spec → rust/niao/markdown artifacts |
| 13 | panic reports | **NEW `ncrash`** — structured JSON crash reports, wrap(fn), fingerprints |
| 14 | nscaffold | **NEW `nscaffold`** — CRUD route + model + migration + test from a struct spec |
| 15 | LSP explain | Roadmap (LSP feature, no lib surface) |
| 16 | typed structured output | **NEW `nschema`** — schema from example, validate/coerce/parse JSON, LLM prompt snippet |
| 17 | nagent tools | EXISTS `nagent` — wired this batch; pair with nschema for tool schemas |
| 18 | nvector | EXISTS `nvec` |
| 19 | neval | **NEW `neval`** — exact/token-F1/similarity, classification+regression metrics, dataset runner |
| 20 | async generators | Roadmap (language); `npipe`/`nlazy` cover pipeline surface |
| 21 | nprompt templating | **NEW `ntemplate`** (name `nprompt` is taken by CLI prompts) — versioned templates, vars, token estimate |
| 22 | nembed cache | **NEW `nembed`** — content-hash embedding cache + local deterministic embedder |
| 23 | nai.guard | **NEW `nguard`** — PII scan/redact (email/phone/ssn/card+Luhn/ip/api-key), denylist |
| 24 | provider registry | **NEW `nprovider`** — provider profiles, model aliases, failover chain, pricing |
| 25 | nai.cost | EXISTS `ncost` |
| 26 | context manager | **NEW `nctx`** — token estimates, trim strategies, budgets, message stats |
| 27 | agent replay | EXISTS — `ncassette` + `nreplay` combination |
| 28 | JIT | Roadmap (VM) |
| 29 | SIMD builtins | **NEW `nsimd`** — unrolled autovectorized f64/i64 kernels on packed arrays |
| 30 | zero-copy io | **NEW `nmmap`** — memory-mapped files (memmap2), lazy line index, byte search |
| 31 | arena allocator | **NEW `narena`** — pooled packed-buffer reuse (recycle/reset), GC-pressure relief |
| 32 | const folding | Roadmap (compiler pass) |
| 33 | SoA layout | **NEW `nsoa`** — columnar struct-of-arrays tables with typed columns |
| 34 | profiler | EXISTS `nprofile` |
| 35 | escape analysis | Roadmap (VM) |
| 36 | inline caching | Roadmap (VM) |
| 37 | parallelism hints | **NEW `npar`** — explicit rayon parallel ops on packed arrays (auto-dispatch = roadmap) |
| 38 | persistent DS | **NEW `npersist`** — im-backed persistent Vector/HashMap with structural sharing |
| 39 | lazy evaluation | **NEW `nlazy`** — fused lazy pipelines (map/filter/take → collect/sum) |
| 40 | warm-start snapshots | **NEW `nsnap`** — fast binary value snapshots + staleness (VM heap snapshot = roadmap) |
| 41 | NUMA/affinity | `npar.set_threads` (pinning = roadmap) |
| 42 | columnar wire format | **NEW `ncolumnar`** — column-major binary codec for tables (magic NCOL1) |

## Error code map

Previously reserved and now wired into codes.rs: batch 2 (2840–2939), unique batch
(2940–3159), AI/HW (2700–2743, plus 2760–2763 neval, 2770–2773 ntok).

New allocations (module files use local consts; codes.rs mirrors them):

```
nconfig    3160 arity, 3161 error, 3162 type, 3163 missing
nbench     3170 arity, 3171 error, 3172 type
ntrace     3180 arity, 3181 error, 3182 type, 3183 invalid handle
ncrash     3190 arity, 3191 error, 3192 type
nhotreload 3200 arity, 3201 error, 3202 type, 3203 invalid handle
ndoc       3210 arity, 3211 error, 3212 type
nlint      3220 arity, 3221 error, 3222 type, 3223 parse
nworkspace 3230 arity, 3231 error, 3232 type
nerrgen    3240 arity, 3241 error, 3242 type
nscaffold  3250 arity, 3251 error, 3252 type
nmigrate   3260 arity, 3261 error, 3262 type
nrepl      3270 arity, 3271 error, 3272 type
ndebug     3280 arity, 3281 error, 3282 type, 3283 invalid handle
nschema    3290 arity, 3291 error, 3292 type, 3293 validate
ntemplate  3300 arity, 3301 error, 3302 type
nembed     3310 arity, 3311 error, 3312 type, 3313 invalid handle
nguard     3320 arity, 3321 error, 3322 type
nprovider  3330 arity, 3331 error, 3332 type
nctx       3340 arity, 3341 error, 3342 type
neval      2760 arity, 2761 error, 2762 type, 2763 shape
ntok       2770 arity, 2771 error, 2772 type, 2773 invalid handle
nsimd      3350 arity, 3351 error, 3352 type
nmmap      3360 arity, 3361 error, 3362 type, 3363 invalid handle
narena     3370 arity, 3371 error, 3372 type, 3373 invalid handle
nsoa       3380 arity, 3381 error, 3382 type, 3383 invalid handle
npar       3390 arity, 3391 error, 3392 type
npersist   3400 arity, 3401 error, 3402 type, 3403 invalid handle
nlazy      3410 arity, 3411 error, 3412 type, 3413 invalid handle
nsnap      3420 arity, 3421 error, 3422 type, 3423 format
ncolumnar  3430 arity, 3431 error, 3432 type, 3433 format
```

## Build protocol (stub-replace, parallel-safe)

The parent pre-creates a **compiling stub** for every new module and wires all of
lib.rs up front, so the tree stays green and each subagent can self-verify.

Each subagent, per assigned lib:
1. Read exemplars first: `crates/niao_runtime/src/nsemver.rs` (small) and
   `crates/niao_runtime/src/nquota.rs` (handle registry pattern).
2. Replace the stub `crates/niao_runtime/src/<lib>.rs` with the full implementation:
   - local `const E####_<LIB>_...` error codes (from the map above)
   - flat builtins `<lib>_<fn>` + namespace short names via the macro pattern
   - `MODULE_NAME`, `MODULE_PATHS = ["<lib>", "std/<lib>"]`, `builtins()`, `namespace()`
   - hard errors → `RuntimeError::at(span, code, ...)`; recoverable → `error_value(code, "<lib>_error", msg, span)`
   - `#[cfg(test)]` unit tests
3. Create `docs/<LIB>.md` (NSEMVER.md style: Import, Quick start, Functions table, Errors table).
4. Create `examples/<lib>_demo.niao` — offline, deterministic, `import "<lib>"`, exercises the main APIs, `fn main()`.
5. Create `niao_libs/<lib>/package.json` and `niao_libs/<lib>/0.2.2/lib.json`
   (same JSON shape as `niao_libs/nsemver/*`, accurate `builtin_count`).
6. `cargo check -p niao_runtime` — fix errors **in your own files only** (other
   agents may be mid-write; their errors are not yours). `cargo test -p niao_runtime <lib>::`.
7. DO NOT touch `lib.rs`, `codes.rs`, `catalog.rs`, `catalog.json`, `Cargo.toml`.

Parent afterwards: `niao_pkg/catalog.rs` + `niao_libs/catalog.json` entries, full
build, run every demo, fix stragglers.

## Subagent groups

1. dx-core: nconfig, nbench, ntrace, ncrash
2. dx-tooling: nlint, ndoc, nerrgen, nscaffold
3. dx-live: nhotreload, nrepl, ndebug, nworkspace
4. data-schema: nmigrate, nschema, nprovider, nctx
5. ai-text: ntemplate, ntok, nguard, neval
6. ai-store: nembed, nsnap, ncolumnar
7. perf-arrays: nsimd, npar, nlazy
8. perf-memory: nmmap, narena, nsoa, npersist
9. hw-docs: docs + examples + niao_libs packages for ncpu, ngpu, nram, nnpu, ndevice (modules already exist)

## New runtime dependencies

- `niao_parser` (workspace) — nlint / ndoc / nhotreload parse Niao source (no cycle: parser → ast/lexer/errors only)
- `memmap2 = "0.9"` — nmmap
- `im = "15"` — npersist structural sharing
- rayon (already present) — npar

## Speed conventions

Packed arrays (`FloatArray`/`IntArray`/`ByteArray`) end-to-end, `thread_local!`
handle registries, no locks on hot paths, `#[inline]` kernels, unrolled
`chunks_exact(8)` loops for autovectorization, preallocated buffers, single-pass
fused pipelines in nlazy, `QuietGuard` while benchmarking.
