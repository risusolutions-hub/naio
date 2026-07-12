# ahiru_core → niao_io migration map (task 09 phase 1)

`ahiru_core` stays on **tokio + axum + hyper + tower-http** for this task. Phase 1 delivers `niao_io` (poller + executor + TCP helpers) and wires `niao_runtime/async_tasks.rs` off the custom mpsc thread pool. This document maps what ahiru uses today and what must grow in `niao_io` + `niao_http` before ahiru can migrate.

## Current ahiru stack (`crates/ahiru_core`)

| Layer | Crate / feature | Used for |
|-------|-----------------|----------|
| Runtime | `tokio` multi-thread | `axum::serve`, handler timeouts, WebSocket pump |
| HTTP server | `axum` + `hyper` | Router, extractors, body streaming, WS upgrade |
| Middleware | `tower`, `tower-http` | CORS, compression (gzip/br), body limit, static files, security headers |
| TLS | `tokio-rustls`, `rustls` | HTTPS listener (keep rustls; do not rewrite) |
| WebSocket | `tokio-tungstenite` | WS frame I/O in `server.rs` |
| Async DB | `sqlx` (`runtime-tokio`) | MySQL/Postgres/SQLite pools |
| Sync DB | `rusqlite`, `postgres`, `r2d2_*` | **Do not rewrite rusqlite** — migrate wire only in task 10 |

## niao_io phase 1 (done)

- WSAPoll / epoll / kqueue poller behind `Poller`
- Work-stealing callback executor (`spawn`, `Executor::global`)
- Timer min-heap (`TimerQueue`, `sleep`)
- mpsc channel wrapper
- TCP connect/listen/accept + readiness wait helpers
- `niao_runtime::async_tasks::spawn_async` → `niao_io::Executor`

## Gap analysis → future work

| ahiru feature | tokio/axum dependency | niao replacement path |
|---------------|----------------------|------------------------|
| `axum::serve(listener, router)` | hyper accept loop | `niao_http` sync server already exists; add async accept loop on `niao_io::Poller` + connection registry |
| Request routing / path params | axum Router | Keep Rust-side router in ahiru; no Niao change |
| `Request` body `to_bytes` | axum/hyper body | `niao_http` incremental body reader + size cap (tower `RequestBodyLimitLayer` equivalent) |
| Handler `timeout()` | `tokio::time::timeout` | `niao_io::TimerQueue` + cancel flag on task |
| WebSocket upgrade | axum `WebSocketUpgrade` + tungstenite | `niao_ws` server accept + frame codec; pump on poller instead of tokio tasks |
| WS mpsc pump | `tokio::sync::mpsc` | `niao_io::channel` + poller-driven read/write |
| CORS / compression / static | tower-http layers | Implement as pre-handler middleware in ahiru using `niao_http` response builder |
| sqlx async pools | tokio runtime | Out of scope phase 1; keep sqlx on tokio until task 10+ or dedicated sqlx migration |
| Graceful shutdown | tokio signal + `serve` handle | `niao_io` wakeup fd + listener deregister |

## Recommended migration order (post task 09)

1. **HTTP only** — point ahiru static routes at `niao_http::Server` + poller-driven accept (no axum for simple apps).
2. **WebSocket** — swap `tokio-tungstenite` for `niao_ws` + `niao_io` readiness.
3. **Middleware** — reimplement CORS/limit/compression as thin ahiru wrappers (no tower).
4. **Full axum removal** — when routing + multipart + WS parity tests pass under load.

## Features explicitly deferred

- Full `Future`/`async fn` language support in Niao VM (callback model sufficient for `spawn_async` today).
- IOCP on Windows (WSAPoll fallback is acceptable phase 1).
- sqlx/tokio removal from ahiru (separate from niao_runtime thread pool).

## Verification before ahiru cutover

- 10k concurrent echo connections (Linux CI) on `niao_io` poller path.
- ahiru `tests/request_throughput.rs` equivalent on niao stack.
- Idle CPU ~0% with no registered sockets (poller sleeps, no busy-loop).
