# Niao Self-Hosting Plan — Eliminating Every External Crate

**Goal:** Niao depends on *zero* third-party crates.io packages. Every capability is
implemented in-house as a `niao_*` crate. The Rust standard library and direct OS
FFI (`extern "C"` to libc / the Windows API) are permitted — they are part of the
platform, not third-party dependencies.

**Status:** In progress. Phase 1 (language core) started 2026-07-13.

> Build constraint: this workspace cannot be compiled inside the assistant's Linux
> sandbox (Rust toolchain + Windows-specific crates). Every change is verified by
> building on Windows (`cargo build` / `cargo test`) and iterating on reported errors.

---

## 1. Current state

The workspace already ships ~50 `niao_*` crates and has re-implemented a large amount
of the usual ecosystem in-house: `niao_crypto`, `niao_http` (client core),
`niao_json_core`, `niao_regex`, `niao_rand`, `niao_bignum`, `niao_time`, `niao_codec`,
`niao_archive`, `niao_collections`, `niao_args` (a clap-style parser), `niao_ws`, and
more. What remains are the external crates listed below.

## 2. Remaining external dependencies (full inventory)

| Crate using it | External dep(s) | Native replacement | Tier |
|---|---|---|---|
| niao_runtime | `serde_json`, `serde` | niao_json_core / niao_serialize | Core |
| niao_runtime | `im-rc` | **niao_persistent** (new) | Core |
| niao_runtime | `rayon` | **niao_parallel** (new) | Core |
| niao_runtime | `memmap2` | niao_io / niao_sys (OS FFI) | Core |
| niao_runtime, niao_http, niao_ws | `rustls`, `rustls-native-certs`, `rustls-pki-types` | niao_tls (new) on niao_crypto | Very hard |
| niao_runtime | `lettre` | niao_smtp (new) | Hard |
| niao_runtime, ahiru_core | `rusqlite` | niao_sql (new) / niao_db | Very hard |
| ahiru_core | `sqlx` | niao_db async drivers | Very hard |
| niao_runtime | `mongodb`, `bson` (opt) | niao_mongo + niao_bson | Very hard |
| niao_runtime, ahiru_core | `tokio`, `futures`, `futures-util` | niao_async (new) | Very hard |
| ahiru_core | `hyper`, `hyper-util` | niao_http (server side) | Hard |
| niao_cli, niao_web, ahiru_core | `axum` | niao_web (router/framework) | Hard |
| ahiru_core | `tower`, `tower-http` | niao_web middleware | Hard |
| ahiru_core | `tokio-rustls`, `rustls-pemfile`, `tokio-tungstenite` | niao_tls + niao_ws async | Very hard |
| ahiru_core | `dashmap` | niao_collections (concurrent) | Medium |
| niao_cli, niao_nm, niao_args(dev) | `clap` | **niao_args** (already exists) | Medium |
| niao_tensor, niao_ml, niao_data, niao_graph, niao_classic | `rand`, `rand_distr` | **niao_rand** (already exists) | Easy |
| niao_tensor, niao_ml, niao_classic, niao_rag | `rayon` | niao_parallel | Medium |
| niao_tensor | `matrixmultiply` | niao_tensor GEMM kernel | Medium |
| niao_collections | `indexmap`, `ahash` (opt) | niao_collections internals | Medium |
| niao_bignum (dev) | `num-bigint`, `num-traits` | niao_bignum (drop dev-dep) | Easy |
| niao_io (dev) | `socket2` | niao_io native socket opts | Easy |
| niao_net_clients (dev) | `suppaftp` | niao_net_clients FTP | Medium |
| niao_pkg, niao_cli (win) | `winreg` | niao_sys (Windows API FFI) | Medium |
| niao_llm, niao_rag | `thiserror` | niao_errors (hand-written) | Easy |
| niao_llm | `candle-core`, `candle-transformers` | niao_tensor + niao_nn | Very hard |
| niao_llm | `tokenizers` | niao_tokenizers (new) | Hard |
| niao_llm | `hf-hub` (vendored) | niao_hub on niao_http | Medium |
| niao_llm | `llama-cpp-2/-sys` (opt) | niao_gguf (new) | Very hard |
| niao_llm | `encoding_rs` (opt) | niao_codec charsets | Medium |
| niao_rag | `fastembed`, `ort` | niao_onnx (new) / niao embeddings | Very hard |
| niao_tensor (dev) | `criterion` | niao_bench (new) | Medium |

## 3. Difficulty tiers

- **Easy** — a native crate already exists or the dep is dev-only: `rand`/`rand_distr`
  → `niao_rand`, `thiserror` → `niao_errors`, `num-bigint`/`socket2` (dev), removal only.
- **Medium** — new but self-contained: `niao_parallel`, `indexmap`/`ahash`/`dashmap`
  internals, `matrixmultiply` GEMM, `winreg` via OS FFI, `clap` → `niao_args` migration.
- **Hard** — large but tractable: `niao_async`, HTTP server in `niao_http`, `axum`
  framework in `niao_web`, `niao_smtp`, `niao_tokenizers`.
- **Very hard / long-horizon** — safety- or hardware-critical, multi-month each:
  `niao_tls` (TLS 1.2/1.3), `niao_jit` (replaces cranelift), `niao_sql` (SQLite-class
  engine), `niao_mongo`, native ML runtime replacing candle/ort/llama.cpp.

## 4. Phased roadmap (leaves first)

Ordering is bottom-up: replace leaf dependencies before the crates that build on them,
so the workspace keeps compiling at each step.

**Phase 1 — Language core (current).** Make `niao_runtime` + `niao_vm` self-hosting
except for the cross-cutting very-hard deps (TLS, SQL, async, JIT) that get their own
phases. Concretely: `niao_parallel` (rayon), `niao_persistent` (im-rc), then
`serde_json`/`serde` → `niao_json_core`/`niao_serialize`, then `memmap2` → native mmap.

**Phase 2 — Utility leaves.** `clap` → `niao_args` (cli, nm); `rand`/`rand_distr` →
`niao_rand` (tensor, ml, data, graph, classic); `thiserror` → `niao_errors`;
`indexmap`/`ahash`/`dashmap` → `niao_collections`; drop `num-bigint`/`socket2` dev-deps;
`winreg` → `niao_sys`; `encoding_rs` → `niao_codec`; `matrixmultiply` → `niao_tensor`.

**Phase 3 — Crypto / TLS.** `niao_tls` on top of `niao_crypto`; retire the `rustls`
stack from `niao_http`, `niao_ws`, `niao_runtime`, `ahiru_core`. `niao_smtp` retires
`lettre`.

**Phase 4 — Async + web.** `niao_async` (reactor + executor + timers) retires
`tokio`/`futures`; HTTP/1.1 + HTTP/2 server in `niao_http` retires `hyper`/`hyper-util`;
`niao_web` router + middleware retires `axum`/`tower`/`tower-http`; async `niao_ws`
retires `tokio-tungstenite`.

**Phase 5 — Data stores.** `niao_sql` (SQLite-class file engine) retires `rusqlite`;
`niao_db` async drivers retire `sqlx`; `niao_mongo` + `niao_bson` retire
`mongodb`/`bson`.

**Phase 6 — JIT.** `niao_jit` (SSA IR → regalloc → x86-64/aarch64 encoders) retires the
`cranelift-*` stack in `niao_vm`. Largest single subproject.

**Phase 7 — ML / AI.** `niao_tensor` + `niao_nn` retire `candle`; `niao_tokenizers`
retires `tokenizers`; `niao_hub` retires `hf-hub`; `niao_onnx` retires `ort`/`fastembed`;
`niao_gguf` retires `llama-cpp`.

**Phase 8 — Tooling.** `niao_bench` retires `criterion`.

## 5. Engineering policy

- **Std + OS FFI are allowed.** Registry access, mmap, sockets, and threads use `std`
  or direct `extern` bindings — no wrapper crates.
- **Safety-critical bring-up runs in parallel.** For `niao_tls`, `niao_sql`, and the JIT,
  keep the existing external crate behind an *optional* Cargo feature during development
  and validate the native implementation against published test vectors and real-world
  interop before flipping the default and deleting the dependency. This is bring-up
  hygiene, not a scope reduction — the end state is still zero external crates.
- **Every step keeps the tree green.** Bottom-up ordering means each phase leaves the
  workspace compiling and testable.

## 6. Progress log

- **2026-07-13** — Plan authored. Phase 1 started: created `niao_parallel` (rayon
  replacement) and `niao_persistent` (im-rc replacement); wired both into
  `niao_runtime` (`npersist.rs`, `npar.rs`, `ncl/parallel.rs`).
- **2026-07-13 (cont.)** — Created `niao_mmap` (read-only memory mapping via OS FFI:
  `mmap`/`munmap` on Unix, `CreateFileMappingW`/`MapViewOfFile` on Windows, whole-file
  `read` fallback elsewhere) and removed `memmap2` from `niao_runtime` (`nmmap.rs`).
  Migrated the hand-built JSON producers in `ncrash.rs` and `ntrace.rs` from `serde_json`
  to `niao_json_core`. Remaining Phase-1 work: finish the mechanical `serde_json` uses in
  `npg`, `ntok`, and `naws`; the coupled ones need more — `nml/data.rs` waits on
  `niao_data` dropping its serde derive, and `nmongo` is tied to `bson` (Phase 5). Once
  all `serde_json` uses are gone, `serde`/`serde_json` leave the runtime entirely.
- **2026-07-13 (cont. 2)** — Migrated `serde_json` → `niao_json_core` in `npg/types.rs`,
  `ntok.rs`, `naws/ssm.rs`, `naws/dynamodb.rs`, and `nllm/handles.rs`. Remaining
  `serde_json` users: `nml/data.rs` (deserializes into `niao_data::PipelineSpec`, which
  still derives serde — the blocker keeping `serde_json` a hard dependency) and
  `nmongo/types.rs` (tied to `bson`, Phase 5). Once `niao_data` drops its serde derive,
  `serde_json` can be gated behind `nmongo` and then removed with Phase 5.
- **2026-07-13 (cont. 3)** — Cleaned `niao_data` of all external crates: dropped its
  `serde` derive (hand-wrote `PipelineSpec::from_json` on `niao_json_core`) and swapped
  `rand` for `niao_rand` in `split.rs`; its deps are now `niao_tensor` + `niao_rand` +
  `niao_json_core`. Migrated `nml/data.rs` to parse via `niao_json_core` +
  `PipelineSpec::from_json`. `serde_json`'s only remaining runtime user is now the
  `bson`-coupled `nmongo` module, so `serde_json` was made an **optional** dependency
  gated behind the `nmongo` feature, and the unused direct `serde` dependency was removed.
  **Default `niao_runtime` builds no longer compile `serde` or `serde_json`.** External
  crates eliminated from the default core so far: `rayon`, `im-rc`, `memmap2`, `serde`,
  `serde_json`.
- **2026-07-13 (cont. 4)** — Extended `niao_rand` with a `Normal`/`Distribution`
  (Box–Muller), `gen_range_f32`/`gen_range_f64`, and a `prelude`, then eliminated `rand`
  and `rand_distr` from **every workspace-member production dependency**: migrated
  `niao_tensor`, `niao_ml` (layer/dataloader/tuning/gnn), `niao_graph`, and `niao_classic`
  (`gen::<f32>()`→`gen_f32()`, `gen_range(0..n)`→`gen_range_usize(0,n)`, `Normal` +
  `shuffle` via `niao_rand`). `rand`/`rand_distr` now survive only in `niao_rand`'s own
  benchmark dev-dependency and the not-yet-wired `niao_ncrypt` crate (outside the
  workspace). Full external-crate eliminations to date: `rayon`, `im-rc`, `memmap2`,
  `serde`, `serde_json` (default), `rand`, `rand_distr`.
- **2026-07-13 (cont. 5)** — Removed `rayon` from every workspace member: added
  `niao_parallel::for_each_mut`; migrated `niao_tensor` (`parallel_add_f32` → `zip_map`),
  `niao_classic` (kmeans + random-forest `par_iter_mut` → `for_each_mut`), and `niao_rag`
  (`into_par_iter().filter_map()` → `map` + flatten); dropped the dead `rayon` dep from
  `niao_ml`. Full eliminations to date: `im-rc`, `memmap2`, `rand`, `rand_distr`,
  `rayon`; plus `serde`/`serde_json` gone from default runtime builds.
- **Discovery:** several `niao_*` crates under `crates/` are **not** listed in the root
  workspace `members` — at least `niao_cal`, `niao_otp`, `niao_ipaddr`, `niao_reflect`,
  `niao_ncrypt`, `niao_retry`. They still use `rayon`/`rand` and are not part of the main
  `cargo build`. They need the same conversion **and** should be added to `members` if
  they're meant to ship as part of niao.
- **2026-07-13 (cont. 6)** — Replaced `matrixmultiply` with a native strided `sgemm`
  (ikj order) in `niao_tensor`; removed `niao_ml`'s dead `serde` dep. **The entire
  numeric/ML/scientific stack is now free of external production crates**: `niao_tensor`
  (only `niao_parallel` + `niao_rand`; `candle-core` is an optional GPU backend and
  `criterion` a dev-only bench dep), `niao_ml`, `niao_data`, `niao_graph`, `niao_classic`,
  plus the already-`niao_*`-only `niao_num`, `niao_stats`, `niao_frame`, `niao_optim`,
  `niao_plot`, `niao_learn`, `niao_boost`, `niao_ts`, `niao_nlp`, `niao_vision`. Remaining
  external deps are now concentrated in four areas: the ML *runtime* (`niao_llm`/`niao_rag`
  — candle/tokenizers/ort/fastembed/hf-hub), the networking/async/DB tier
  (`niao_http`/`niao_ws`/`niao_runtime`/`ahiru_core` — rustls/tokio/hyper/axum/tower/
  lettre/rusqlite/sqlx/mongodb/bson/futures/dashmap), the JIT (`niao_vm` — cranelift), and
  a mechanical remainder (`clap` in `niao_cli`/`niao_nm`, `winreg` in `niao_pkg`, optional
  `indexmap`/`ahash` in `niao_collections`, `thiserror`, `encoding_rs`, and dev-deps).
- **2026-07-13 (cont. 7)** — Eliminated `thiserror` workspace-wide: hand-wrote
  `Display` + `Error::source` + `From` impls for `RagError` (`niao_rag`) and `LlmError`
  (`niao_llm`). **Full external-crate eliminations to date (gone from every workspace
  production dependency): `im-rc`, `memmap2`, `rand`, `rand_distr`, `rayon`,
  `matrixmultiply`, `thiserror` — 7 crates**; plus `serde`/`serde_json` removed from
  default runtime builds. Deferred deliberately: `winreg` (registry/PATH FFI is sensitive
  — kept until a `niao_winreg` layer can be tested per the bring-up policy).
