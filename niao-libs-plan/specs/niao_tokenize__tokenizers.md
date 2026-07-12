# Library spec: `tokenizers`  →  crate `niao_tokenize`

| | |
|---|---|
| Category | ML |
| Replaces Rust crate(s) | `tokenizers` (v0.21) |
| Target Niao crate | `crates/niao_tokenize` |
| Difficulty | 4/5 — Very Hard |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | json |

## Goal
Reimplement the functionality of `tokenizers` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
BPE/WordPiece/Unigram

## Implementation blueprint (make it FAST + LIGHT)
load HF tokenizer.json, implement BPE merges + WordPiece + Unigram viterbi, byte-level pretokenizer, special tokens, decode; trie/heap for merges.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`Tokenizer::from_file/encode/decode`
Expose to Niao programs through a `niao_libs/tokenize/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
>= tokenizers on typical text

## Tests required
identical ids vs HF fixtures for a few models
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
must match HF exactly or models break

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
