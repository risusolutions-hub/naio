# Task 08 — nws: own WebSocket, RFC 6455 (replace tungstenite in niao_runtime)
Read MASTER_PLAN.md first.

## Build
Create `crates/niao_ws` (deps: niao_http, niao_crypto, niao_codec):
- Client + server handshake (Sec-WebSocket-Key/Accept via sha1 — add SHA-1 to niao_crypto, marked handshake-only).
- Frame codec: masking, fragmentation, ping/pong/close, 125/64k/64bit lengths, UTF-8 validation on text.
- Sync API over TcpStream/rustls stream matching how niao_runtime/net uses tungstenite today.

## Wire up
- Replace `tungstenite` in niao_runtime. ahiru_core keeps tokio-tungstenite until task 09 lands its async story — note it in the report.

## Acceptance
- Autobahn-style unit cases (fragmented text, interleaved ping, invalid close codes, unmasked client frame rejected server-side).
- Echo integration test client<->server; bench message throughput vs tungstenite.

## Ground rules (apply to EVERY task)
- ZERO new third-party crates. Only std + existing niao_* workspace crates.
- Lightweight & fast: no allocations in hot loops, prefer &str/&[u8], pre-size Vecs, add #[inline] on small hot fns.
- Public API surface goes to Niao programs via niao_runtime builtins + a niao_libs/<name>/ module (Niao-language wrapper), mirroring how niao_libs/json works today.
- Add unit tests in the crate + at least one .niao example under examples/.
- Add/extend a benchmark under benchmarks/ comparing before vs after.
- After changes: `cargo check --workspace` then `cargo test --workspace` must pass.
- Remove the replaced third-party dependency from every Cargo.toml that used it, and confirm it disappears from `cargo tree`.
- Update CHANGELOG.md with one line.
