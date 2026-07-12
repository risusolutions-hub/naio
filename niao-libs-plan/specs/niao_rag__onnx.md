# Library spec: `onnx`  →  crate `niao_rag`

| | |
|---|---|
| Category | ML |
| Replaces Rust crate(s) | `ort,fastembed` (v2.0) |
| Target Niao crate | `crates/niao_rag` |
| Difficulty | 5/5 — Extreme |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | tensor, tokenize |

## Goal
Reimplement the functionality of `ort,fastembed` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
ONNX runtime

## Implementation blueprint (make it FAST + LIGHT)
ONNX graph loader + operator set for embedding models (subset used by fastembed), run on niao_tensor; or keep ort FFI.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`embed(text)->vec`
Expose to Niao programs through a `niao_libs/rag/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
within 3x ort CPU

## Tests required
embeddings cosine-match reference
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
ENORMOUS; ONNX opset is large. Recommend keeping ort or implementing only the ops fastembed models need.

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
