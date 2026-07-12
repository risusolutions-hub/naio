# Task 09 — nio: our own async foundation (tokio exit strategy, phase 1)
Read MASTER_PLAN.md first. Do NOT try to clone tokio. Phase 1 = a solid readiness-based event loop + structured tasks that niao_runtime/ahiru can target.

## Build
Create `crates/niao_io` (zero deps, unsafe allowed but minimal, cfg per-OS):
- Poller: epoll (linux), kqueue (macos), IOCP or WSAPoll fallback (windows) behind one trait.
- Non-blocking TcpListener/TcpStream registration, read/write readiness, timers (binary heap), wakeups (eventfd/pipe/completion).
- Executor: N worker threads, global injector + per-worker deque (simple work-stealing), our own lightweight Future-less callback/coroutine model OR std::future support — pick whichever fits niao_runtime/async_tasks.rs best and justify in the report.
- Provide: spawn, sleep, tcp accept/connect/read/write, channel (mpsc).

## Wire up
- Port niao_runtime/async_tasks.rs to niao_io.
- ahiru_core stays on tokio/hyper/axum THIS task; produce a written migration map (which axum/tower features ahiru actually uses → what niao_io+niao_http must grow) as MIGRATION_ahiru.md.

## Acceptance
- Stress test: 10k concurrent echo connections on linux CI profile; timer accuracy test; no busy-loop (CPU ~0 when idle).
- Existing Niao async examples/tests still pass.

## Ground rules (apply to EVERY task)
- ZERO new third-party crates. Only std + existing niao_* workspace crates.
- Lightweight & fast: no allocations in hot loops, prefer &str/&[u8], pre-size Vecs, add #[inline] on small hot fns.
- Public API surface goes to Niao programs via niao_runtime builtins + a niao_libs/<name>/ module (Niao-language wrapper), mirroring how niao_libs/json works today.
- Add unit tests in the crate + at least one .niao example under examples/.
- Add/extend a benchmark under benchmarks/ comparing before vs after.
- After changes: `cargo check --workspace` then `cargo test --workspace` must pass.
- Remove the replaced third-party dependency from every Cargo.toml that used it, and confirm it disappears from `cargo tree`.
- Update CHANGELOG.md with one line.
