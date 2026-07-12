# Library spec: `io-async`  →  crate `niao_io`

| | |
|---|---|
| Category | Async |
| Replaces Rust crate(s) | `(tokio phase1)` (vn/a) |
| Target Niao crate | `crates/niao_io` |
| Difficulty | 5/5 — Extreme |
| Status | ALREADY BUILT (tasks 01-12) — verify/extend only |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `(tokio phase1)` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
epoll/kqueue/IOCP

## Implementation blueprint (make it FAST + LIGHT)
readiness poller per-OS, non-blocking TCP, timer heap, work-stealing executor, mpsc channel.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`spawn/sleep/tcp/channel`
Expose to Niao programs through a `niao_libs/io/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
10k conns, idle ~0 CPU

## Tests required
stress echo
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
IOCP deferred (WSAPoll)

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
