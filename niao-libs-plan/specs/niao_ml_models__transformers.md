# Library spec: `transformers`  →  crate `niao_ml_models`

| | |
|---|---|
| Category | ML |
| Replaces Rust crate(s) | `candle-transformers` (v0.8) |
| Target Niao crate | `crates/niao_ml_models` |
| Difficulty | 5/5 — Extreme |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | tensor, tokenize |

## Goal
Reimplement the functionality of `candle-transformers` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
model architectures

## Implementation blueprint (make it FAST + LIGHT)
implement Llama/BERT/etc forward passes on niao_tensor, load safetensors/gguf weights.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`model.forward`
Expose to Niao programs through a `niao_libs/ml_models/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
usable inference

## Tests required
logits match reference
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
HUGE; per-architecture work

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
