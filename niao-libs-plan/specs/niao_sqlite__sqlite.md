# Library spec: `sqlite`  →  crate `niao_sqlite`

| | |
|---|---|
| Category | Database |
| Replaces Rust crate(s) | `rusqlite,r2d2_sqlite` (v0.32) |
| Target Niao crate | `crates/niao_sqlite` |
| Difficulty | 5/5 — Extreme |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `rusqlite,r2d2_sqlite` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
SQLite file format + SQL

## Implementation blueprint (make it FAST + LIGHT)
Either keep the SQLite C lib via FFI (recommended), or implement the SQLite file format reader/writer + B-tree + a SQL subset. Full native SQLite = massive.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`sqlite.open/exec/query`
Expose to Niao programs through a `niao_libs/sqlite/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
n/a

## Tests required
CRUD, existing nsqlite suite
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
HUGE if native. Recommend FFI to bundled sqlite3.c (single-file amalgamation), removes the crate but keeps C core.

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
