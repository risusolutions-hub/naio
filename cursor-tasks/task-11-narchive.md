# Task 11 — narchive: own deflate/gzip/tar/zip (replace flate2, tar, zip)
Read MASTER_PLAN.md first.

## Build
Create `crates/niao_archive` (deps: niao_crypto for crc32? no — write crc32 here; zero external):
- inflate (RFC 1951 decode) + gzip wrapper (RFC 1952) — decode first, it unblocks package installs.
- deflate encode: fixed-huffman + LZ77 greedy first (correct > optimal), stored-block fallback; document ratio gap vs flate2.
- crc32 (slice-by-8), adler32.
- tar: ustar+pax read/write (what niao_pkg needs).
- zip: read + write, stored + deflate methods, zip64 read.

## Wire up
- niao_pkg: package pack/unpack (tar.gz + zip) onto niao_archive; nhttp client gains gzip response decoding (finishes task 07 TODO).

## Acceptance
- Round-trip tests; cross-check fixtures created by the old crates (generate fixture files BEFORE removing them, commit under tests/fixtures/).
- Bench: inflate speed >= 60% of flate2 (miniz is very optimized; that's acceptable v1), correctness 100%.

## Ground rules (apply to EVERY task)
- ZERO new third-party crates. Only std + existing niao_* workspace crates.
- Lightweight & fast: no allocations in hot loops, prefer &str/&[u8], pre-size Vecs, add #[inline] on small hot fns.
- Public API surface goes to Niao programs via niao_runtime builtins + a niao_libs/<name>/ module (Niao-language wrapper), mirroring how niao_libs/json works today.
- Add unit tests in the crate + at least one .niao example under examples/.
- Add/extend a benchmark under benchmarks/ comparing before vs after.
- After changes: `cargo check --workspace` then `cargo test --workspace` must pass.
- Remove the replaced third-party dependency from every Cargo.toml that used it, and confirm it disappears from `cargo tree`.
- Update CHANGELOG.md with one line.
