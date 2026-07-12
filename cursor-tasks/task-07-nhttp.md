# Task 07 — nhttp: own HTTP/1.1 stack (replace httparse, tiny_http, ureq, http types)
Read MASTER_PLAN.md first. Scope = HTTP/1.1 only; HTTP/2 stays on hyper for ahiru (for now).

## Build
Create `crates/niao_http` (deps: std, niao_codec; TLS via rustls which stays):
- types: Method, Status, HeaderMap (lowercase keys, small-vec), Request/Response builders.
- parser: incremental request/response parser (state machine over &[u8], no copies until body), chunked transfer decoding, content-length, header limits.
- client (sync): connection pool, keep-alive, redirects, gzip decode later (after task 11), rustls for https, timeouts. API like ureq: nhttp::get(url).header(..).send()?.
- server (sync, thread-pool): replacement for tiny_http usage in niao_runtime/net — accept loop + worker threads, keep-alive, graceful shutdown.

## Wire up
- niao_runtime: replace httparse, tiny_http, ureq, url (write a small URL parser in this crate too — scheme/host/port/path/query/fragment + percent-encoding).
- niao_pkg + niao_rag: switch ureq download calls to nhttp client.
- Keep Niao-facing `net` lib API identical.

## Acceptance
- Parser fuzz-ish tests (truncated, folded headers rejected, smuggling patterns: dual Content-Length, CL+TE => reject).
- Integration test: nhttp client against nhttp server, 10k keep-alive requests.
- Bench: requests/sec >= tiny_http on hello-world workload; client latency ~ ureq.

## Ground rules (apply to EVERY task)
- ZERO new third-party crates. Only std + existing niao_* workspace crates.
- Lightweight & fast: no allocations in hot loops, prefer &str/&[u8], pre-size Vecs, add #[inline] on small hot fns.
- Public API surface goes to Niao programs via niao_runtime builtins + a niao_libs/<name>/ module (Niao-language wrapper), mirroring how niao_libs/json works today.
- Add unit tests in the crate + at least one .niao example under examples/.
- Add/extend a benchmark under benchmarks/ comparing before vs after.
- After changes: `cargo check --workspace` then `cargo test --workspace` must pass.
- Remove the replaced third-party dependency from every Cargo.toml that used it, and confirm it disappears from `cargo tree`.
- Update CHANGELOG.md with one line.
