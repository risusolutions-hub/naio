# Changelog

## Unreleased

### Added
- **ML Wave 0 — `nnum`**: `niao_num` crate (NdArray, broadcasting, linalg, FFT), native `nnum` module, `docs/NNUM.md`, `examples/nnum_demo.niao`, error codes 4000–4009.
- **ML Wave 1 — `nframe`, `nstats`, `noptim`, `nplot`**: four crates + native modules, docs/demos/benchmarks, error codes 4010–4049.
- **ML Wave 2 — `nlearn`, `nboost`, `nts`, `nnlp`, `nvision`**: five crates, docs/demos/benchmarks, error codes 4050–4099 (runtime module wiring partial — expand in follow-up).

## 0.2.3 — 2026-07-13

### Release
- Toolchain (`niao`, `nm`) and distribution bundles bumped to **0.2.3**.
- Standard library packages aligned to **0.2.3** (111 libs; `ahiru` remains at **0.3.0**).
- Registry seeded with **113** packages (223 version tarballs including 0.2.2 + 0.2.3).
- Windows x64/x86/ARM64 release artifacts published to `nm.c4compare.com`.

### Added
- `niao_codec` crate (zero-deps base64, hex, UUID v4/v7, dotenv) with `codec` Niao module; replaces `base64`, `uuid`, and `dotenvy` in `ahiru_core` / `niao_runtime`.
- `niao_json_core` crate (zero-deps JSON parse/stringify); wired into `niao_runtime` JSON builtins; replaces direct `serde_json` use in JSON hot paths.
- TOML parser in `niao_json_core::toml`; replaces `toml` crate in `ahiru_core` / `niao_cli` config paths.
- `niao_crypto` crate (SHA-256/512, HMAC, JWT HS256/HS512); replaces `sha2`, `hmac`, `jsonwebtoken` in `ahiru_core` and `sha2` in `niao_pkg`.
- `niao_regex` crate (Thompson NFA + Pike VM); replaces `regex` in `niao_runtime` `re` module with pattern LRU cache.
- `niao_time` crate (civil date math, RFC3339/RFC2822, strftime subset, compact IANA tz); replaces `chrono` / `chrono-tz` in `niao_runtime` `time` module.
- `niao_http` crate (HTTP/1.1 parser, sync client/server, URL utilities); replaces `ureq`, `httparse`, `tiny_http`, and direct `url` in `niao_runtime` / `niao_pkg`.
- `niao_ws` crate (RFC 6455 WebSocket client/server); replaces `tungstenite` in `niao_runtime`; SHA-1 added to `niao_crypto` for handshake only.
- `niao_io` crate (readiness poller, work-stealing executor, timers, TCP helpers); replaces custom mpsc thread pool in `niao_runtime::async_tasks`.
- `niao_db` crate (RESP Redis client, PostgreSQL wire v3, generic pool); replaces `postgres`, `r2d2`, `r2d2_postgres`, and `redis` in `niao_runtime`/`ahiru_core` npg paths.
- `niao_archive` crate (RFC 1951 deflate, gzip, ustar/PAX tar, zip stored+deflate); replaces `flate2`, `tar`, and `zip` in `niao_pkg`; gzip response decoding in `niao_http`.
- `niao_args` crate (zero-deps CLI parser); clap-parity tests for `niao`/`nm` command trees; replaces `clap` in `niao_cli` / `niao_nm` (migration pending orchestrator pass).
- **Wave 0** native stdlib crates: `niao_collections` (ahash + indexmap), `niao_bignum` (num-bigint), `niao_rand`, `niao_log` (tracing), `niao_net_clients` (suppaftp FTP); `niao_http` extended with http-types + URL modules.
- Wave 0 orchestrator: wired `niao_runtime` to `niao_bignum`, `niao_collections`, `niao_io` (socket), `niao_net_clients` (FTP); registered `nargs`, `nlog`, `nmath`, `nrand`, `nstr`, `nfmt` native modules; `ahiru_core` logging via `niao_log`; added `niao_log` workspace member.
- VM/runtime polish: bytecode cache content-hash sidecar, call-bridge arg scratch buffer, GC threshold tune, hot-path `get_unchecked` in dispatch; `scripts/bench_gate.ps1` + `benchmarks/baseline.json`; `docs/perf_notes.md`.
- Task 13 verification: CMake installed (Kitware 4.4.0); full `cargo check/test --workspace` green; `niao_pkg`/`niao_bytecode` migrated off direct `serde_json`; lexer `//` disambiguation for comments vs floor-div; slow VM/interpreter tests fixed (no `#[ignore]`).

## 0.2.2 — 2026-07-07

### Release
- Toolchain (`niao`, `nm`) and distribution bundles bumped to **0.2.2**.
- Standard library packages aligned to **0.2.2** (`core`, `dsa`, `json`, `io`, `re`, `net`, `parallel`, `time`, `nsqlite`, `npg`, `nmongo`, `nos`, `nenv`, `ncl`, `nml`, `nvis`).
- `ahiru` remains at **0.3.0**.

## 0.2.1 — 2026-07-06

### Performance
- VM, bytecode compiler, tensor runtime, and CLI startup optimizations across `niao_vm`, `niao_bytecode`, `niao_tensor`, `niao_runtime`, and `niao_cli`.

### Fixes
- Windows MSVC release builds link cleanly with `/NODEFAULTLIB:libcpmt.lib` (CRT mismatch with `libort_sys` / `libesaxx_rs`).
- Bytecode wire-format test checks magic header and roundtrip (wire container may be larger than pure JSON when OOP metadata is embedded).

### Notes
- `BYTECODE_CACHE_VERSION` remains **10** — bytecode format unchanged; existing `.niaobc` caches remain valid.
