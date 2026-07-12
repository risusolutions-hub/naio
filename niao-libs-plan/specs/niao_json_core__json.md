# Library spec: `json`  →  crate `niao_json_core`

| | |
|---|---|
| Category | Serialization |
| Replaces Rust crate(s) | `serde_json` (v1) |
| Target Niao crate | `crates/niao_json_core` |
| Difficulty | 3/5 — Hard |
| Status | ALREADY BUILT (tasks 01-12) — verify/extend only |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `serde_json` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
RFC 8259

## Implementation blueprint (make it FAST + LIGHT)
byte-level single-pass parser, insertion-ordered small-map objects, int fast-path, reusable output buffer, Cow zero-copy strings.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`parse/to_string/to_string_pretty/Value`
Expose to Niao programs through a `niao_libs/json_core/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
>= serde_json on 5MB doc

## Tests required
JSONTestSuite subset
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
serde-derive users remain (niao_pkg, niao_bytecode)

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
