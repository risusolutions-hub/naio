# Niao Self-Hosted Libraries — MASTER PLAN

Goal: reimplement **every** third-party Rust library in `niao_rust_deps.txt` as a native,
zero-dependency, lightweight, high-performance Niao/Rust crate. Detailed per-library specs
are in `specs/` (one MD per library). This file is the map + execution order.

Total library specs: **50**  |  Grouped into **30** target crates.

## Reality tiers — read before you start
Some libraries are safe to rewrite; a few are enormous or security-critical. The specs still
cover all of them because you asked for all, but heed the risk notes:

- **GREEN (rewrite freely):** codec, json, toml, crypto(hash/hmac/jwt), regex, time, http/1.1,
  ws, io event-loop, redis/pg wire, archive, collections, rand, bignum, clap, tracing, smtp/ftp,
  hf-hub. Most are done or low-risk.
- **AMBER (large but doable):** serde bridge, rayon, futures, encoding, gemm, tokenizers, hyper,
  tower, axum, brotli, sqlx, mongodb. Real engineering; do one at a time, keep old crate until green.
- **RED (recommend NOT hand-rolling — extreme risk):**
  - `niao_tls` (rustls) — hand-rolled TLS/crypto = remote-exploit risk. Keep rustls; if pursued, external audit mandatory.
  - `niao_jit` (cranelift) — multi-arch codegen is person-years; interpreter fallback must always exist.
  - `niao_tensor`/`transformers`/`llama`/`onnx` — competing with candle/llama.cpp/ONNX C++; FFI wrapper is the pragmatic path.
  - `niao_sqlite` (rusqlite) — native SQLite is massive; bundle sqlite3 amalgamation via FFI instead.

## Target crates
- `crates/niao_archive` ← archive, brotli
- `crates/niao_args` ← clap
- `crates/niao_bignum` ← num-bigint
- `crates/niao_codec` ← base64, dotenv, uuid
- `crates/niao_collections` ← ahash, dashmap, indexmap
- `crates/niao_crypto` ← hmac, jwt, sha2
- `crates/niao_db` ← db, redis-client, sqlx
- `crates/niao_encoding` ← encoding_rs
- `crates/niao_http` ← http, http-types, hyper, url
- `crates/niao_hub` ← hf-hub
- `crates/niao_io` ← futures, io-async, socket2
- `crates/niao_jit` ← jit
- `crates/niao_json_core` ← bson, json, toml
- `crates/niao_llm` ← llama
- `crates/niao_log` ← tracing
- `crates/niao_ml_models` ← transformers
- `crates/niao_net_clients` ← lettre, suppaftp
- `crates/niao_parallel` ← rayon
- `crates/niao_rag` ← onnx
- `crates/niao_rand` ← rand, rand-distr
- `crates/niao_regex` ← regex
- `crates/niao_runtime` ← mongodb
- `crates/niao_serde` ← serde
- `crates/niao_sqlite` ← sqlite
- `crates/niao_tensor` ← matrixmultiply, tensor
- `crates/niao_time` ← time
- `crates/niao_tls` ← tls
- `crates/niao_tokenize` ← tokenizers
- `crates/niao_web2` ← axum, tower
- `crates/niao_ws` ← ws

## Parallel execution WAVES
Libraries in the same wave have no dependencies on each other → run them in **parallel agents**.
Each wave must be green before the next starts (later waves consume earlier crates).

### Wave 0
- ⬜ `ahash` → `niao_collections` (diff 2/5) — replaces `ahash`
- ✅ `archive` → `niao_archive` (diff 4/5) — replaces `flate2,tar,zip`
- ✅ `base64` → `niao_codec` (diff 1/5) — replaces `base64,hex`
- ⬜ `clap` → `niao_args` (diff 2/5) — replaces `clap`
- ✅ `dotenv` → `niao_codec` (diff 1/5) — replaces `dotenvy`
- 🔴 `encoding_rs` → `niao_encoding` (diff 4/5) — replaces `encoding_rs`
- ⬜ `http-types` → `niao_http` (diff 2/5) — replaces `http`
- ⬜ `indexmap` → `niao_collections` (diff 2/5) — replaces `indexmap`
- ✅ `io-async` → `niao_io` (diff 5/5) — replaces `(tokio phase1)`
- 🔴 `jit` → `niao_jit` (diff 5/5) — replaces `cranelift-codegen,cranelift-frontend,cranelift-jit,cranelift-module,cranelift-native`
- ✅ `json` → `niao_json_core` (diff 3/5) — replaces `serde_json`
- 🔴 `matrixmultiply` → `niao_tensor` (diff 4/5) — replaces `matrixmultiply`
- ⬜ `num-bigint` → `niao_bignum` (diff 3/5) — replaces `num-bigint,num-traits`
- ⬜ `rand` → `niao_rand` (diff 2/5) — replaces `rand`
- ✅ `regex` → `niao_regex` (diff 4/5) — replaces `regex`
- ✅ `sha2` → `niao_crypto` (diff 2/5) — replaces `sha2`
- ⬜ `socket2` → `niao_io` (diff 2/5) — replaces `socket2`
- 🔴 `sqlite` → `niao_sqlite` (diff 5/5) — replaces `rusqlite,r2d2_sqlite`
- ⬜ `suppaftp` → `niao_net_clients` (diff 2/5) — replaces `suppaftp`
- ✅ `time` → `niao_time` (diff 3/5) — replaces `chrono,chrono-tz`
- ⬜ `tracing` → `niao_log` (diff 2/5) — replaces `tracing,tracing-subscriber`
- ⬜ `url` → `niao_http` (diff 2/5) — replaces `url`
- ✅ `uuid` → `niao_codec` (diff 1/5) — replaces `uuid`

### Wave 1
- 🔴 `brotli` → `niao_archive` (diff 4/5) — replaces `(tower-http br)`
- ⬜ `bson` → `niao_json_core` (diff 2/5) — replaces `bson`
- ⬜ `dashmap` → `niao_collections` (diff 3/5) — replaces `dashmap`
- ✅ `db` → `niao_db` (diff 4/5) — replaces `redis,postgres,r2d2,r2d2_postgres`
- 🔴 `futures` → `niao_io` (diff 4/5) — replaces `futures,futures-util`
- ✅ `hmac` → `niao_crypto` (diff 1/5) — replaces `hmac`
- ✅ `http` → `niao_http` (diff 4/5) — replaces `httparse,tiny_http,ureq,url,http`
- ⬜ `rand-distr` → `niao_rand` (diff 2/5) — replaces `rand_distr`
- 🔴 `rayon` → `niao_parallel` (diff 4/5) — replaces `rayon`
- ✅ `redis-client` → `niao_db` (diff 2/5) — replaces `redis`
- 🔴 `serde` → `niao_serde` (diff 4/5) — replaces `serde`
- 🔴 `tls` → `niao_tls` (diff 5/5) — replaces `rustls,rustls-pemfile,rustls-native-certs,rustls-pki-types,tokio-rustls`
- 🔴 `tokenizers` → `niao_tokenize` (diff 4/5) — replaces `tokenizers`
- ✅ `toml` → `niao_json_core` (diff 2/5) — replaces `toml`

### Wave 2
- ⬜ `hf-hub` → `niao_hub` (diff 2/5) — replaces `hf-hub`
- 🔴 `hyper` → `niao_http` (diff 5/5) — replaces `hyper,hyper-util`
- ✅ `jwt` → `niao_crypto` (diff 2/5) — replaces `jsonwebtoken`
- ⬜ `lettre` → `niao_net_clients` (diff 2/5) — replaces `lettre`
- 🔴 `mongodb` → `niao_runtime` (diff 4/5) — replaces `mongodb`
- 🔴 `sqlx` → `niao_db` (diff 4/5) — replaces `sqlx`
- 🔴 `tensor` → `niao_tensor` (diff 5/5) — replaces `candle-core`
- ✅ `ws` → `niao_ws` (diff 3/5) — replaces `tungstenite`

### Wave 3
- 🔴 `llama` → `niao_llm` (diff 5/5) — replaces `llama-cpp-2,llama-cpp-sys-2`
- 🔴 `onnx` → `niao_rag` (diff 5/5) — replaces `ort,fastembed`
- 🔴 `tower` → `niao_web2` (diff 3/5) — replaces `tower,tower-http`
- 🔴 `transformers` → `niao_ml_models` (diff 5/5) — replaces `candle-transformers`

### Wave 4
- 🔴 `axum` → `niao_web2` (diff 4/5) — replaces `axum`

Legend: ✅ already built (verify/extend) · ⬜ to build · 🔴 to build, high-risk (read spec risk note)

## Global ground rules (every library)
1. ZERO new third-party crates — only `std` + existing `niao_*`.
2. Lightweight + fast: no hot-loop allocations, reuse buffers, `#[inline]` hot fns, SIMD with scalar fallback.
3. Expose to Niao via `niao_libs/<name>/` wrapper + runtime builtins (mirror `niao_libs/json`).
4. Tests + one `.niao` example + a benchmark vs the replaced crate (generate fixtures BEFORE removing it).
5. Remove replaced crate from all `Cargo.toml`s; confirm gone via `cargo tree`.
6. `cargo check --workspace` + `cargo test --workspace` green before commit. Update CHANGELOG + REPORT.md.
7. Never delete rustls/cranelift/ML-FFI/sqlite C core until the native replacement passes full interop tests.
