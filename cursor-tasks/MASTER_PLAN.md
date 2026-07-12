# Niao Self-Hosted Libraries — Master Plan
Goal: replace third-party Rust crates with our own lightweight, high-speed implementations
(Rust core crates + Niao-language wrappers in niao_libs/), and polish the VM/runtime.

## Strategy
- Perf-critical infrastructure = our own Rust crates (niao_codec, niao_http, ...), zero external deps.
- Developer-facing API = Niao modules in niao_libs/ calling runtime builtins (same pattern as niao_libs/json).
- KEEP as-is (do NOT rewrite — security/FFI reality):
  - rustls (never hand-roll TLS crypto), cranelift-* (JIT backend),
    candle/llama-cpp/ort/tokenizers (C++/GPU FFI), rusqlite (SQLite C binding).
- SKIP: criterion (dev-only), thiserror/num-traits (Rust glue).

## Execution order (run tasks 01→12 with cursor-agent, review git diff after each)
01 foundations: base64/hex/uuid/dotenv          → drops base64, uuid, dotenvy
02 njson core: own JSON engine                  → drops serde_json (phased)
03 config: own TOML parser                      → drops toml
04 crypto: sha2/hmac/jwt                        → drops sha2, hmac, jsonwebtoken, base64 leftovers
05 nregex: own regex engine                     → drops regex
06 ntime: own date/time + tz                    → drops chrono, chrono-tz
07 nhttp: own HTTP/1.1 client+server            → drops httparse, tiny_http, ureq, http (phased)
08 nws: own WebSocket (RFC 6455)                → drops tungstenite, tokio-tungstenite (server side later)
09 nio: own async event loop (tokio exit plan)  → long-term; starts with a thread-pool + readiness loop
10 ndb: RESP (redis) + Postgres wire protocol   → drops redis, postgres, r2d2, r2d2_postgres
11 narchive: deflate/tar/zip                    → drops flate2, tar, zip
12 VM & runtime perfection: GC, dispatch, caching, benchmark CI gates

## Definition of "perfect"
Every task ends with: green workspace tests, benchmark equal-or-faster than the crate it
replaces on our workloads, and the old crate gone from Cargo.lock.

## Ground rules (apply to EVERY task)
- ZERO new third-party crates. Only std + existing niao_* workspace crates.
- Lightweight & fast: no allocations in hot loops, prefer &str/&[u8], pre-size Vecs, add #[inline] on small hot fns.
- Public API surface goes to Niao programs via niao_runtime builtins + a niao_libs/<name>/ module (Niao-language wrapper), mirroring how niao_libs/json works today.
- Add unit tests in the crate + at least one .niao example under examples/.
- Add/extend a benchmark under benchmarks/ comparing before vs after.
- After changes: `cargo check --workspace` then `cargo test --workspace` must pass.
- Remove the replaced third-party dependency from every Cargo.toml that used it, and confirm it disappears from `cargo tree`.
- Update CHANGELOG.md with one line.
