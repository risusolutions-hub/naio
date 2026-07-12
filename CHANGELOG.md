# Changelog

## Unreleased

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
