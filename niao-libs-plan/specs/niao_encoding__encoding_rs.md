# Library spec: `encoding_rs`  →  crate `niao_encoding`

| | |
|---|---|
| Category | Text |
| Replaces Rust crate(s) | `encoding_rs` (v0.8) |
| Target Niao crate | `crates/niao_encoding` |
| Difficulty | 4/5 — Very Hard |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `encoding_rs` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
WHATWG Encoding

## Implementation blueprint (make it FAST + LIGHT)
UTF-8 validate/repair, UTF-16 LE/BE, windows-1252, ISO-8859-*, Shift_JIS/EUC/GBK/Big5 via compact index tables (generate from WHATWG indexes, commit tables). Decoder/encoder streaming.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`decode/encode(label, bytes)`
Expose to Niao programs through a `niao_libs/encoding/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
fast UTF-8 SIMD path

## Tests required
WHATWG encoding tests
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
LARGE index tables; scope to encodings niao_llm actually needs

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
