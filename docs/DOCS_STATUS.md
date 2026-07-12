# Missing-docs status (15 libs)

The audit found 15 libs in `niao_libs/` with no matching file in `docs/`. They split into two groups.

## Group 1 — genuinely undocumented, deserve real docs (10)

| Lib | Source | Doc status |
|---|---|---|
| `io` | `runtime/io.rs` (55 builtins) | ✅ **drafted** → `IO.md` |
| `nos` | `runtime/nos.rs` (24 builtins) | ✅ **drafted** → `NOS.md` |
| `core` | `lib.rs` (print/len/type/assert/int/float/bool/error/…) | ✅ **stub** → `CORE.md` |
| `bignum` | crate `niao_bignum` (8 builtins) | ⏳ TODO — enumerate crate exports, then write |
| `crypto` | crate `niao_crypto` (SHA-256/512, HMAC) | ⏳ TODO |
| `archive` | crate `niao_archive` (gzip/deflate) | ⏳ TODO |
| `http` | crate `niao_http` (2,715 LOC — Method/Status/HeaderMap/Uri) | ⏳ TODO — large; align with `net` doc |
| `net_clients` | crate `niao_net_clients` (SMTP/FTP, 8 builtins) | ⏳ TODO — likely fold into `nsmtp` doc |
| `nllm` | `runtime/nllm/` (13 builtins) | ⏳ TODO — GGUF inference |
| `nrag` | `runtime/nrag/` (15 builtins) | ⏳ TODO — vector RAG |

These four (`bignum`, `crypto`, `archive`, `nllm`/`nrag`) were left as TODO rather than drafted
because their APIs live in dedicated crates / multi-file modules whose exact exported builtin names
were not fully enumerated in this pass — writing them from a guess would risk inaccurate docs. Each
needs a 5-minute `grep` of its registration to pin the exact surface, then the same template as
`IO.md`/`NOS.md`.

## Group 2 — superseded aliases, need a "moved" stub, not a full doc (5)

| Legacy lib | Canonical replacement | Suggested stub |
|---|---|---|
| `args` | `nargs` (documented) | "Use `nargs`. `args` is the legacy crate-backed parser." |
| `log` | `nlog` (documented) | "Use `nlog`. `log` is the `niao_log` backend." |
| `rand` | `nrand` (documented) | "Use `nrand` (xoshiro256\*\*). `rand` is legacy." |
| `codec` | `nfmt` / `ncanon` (base64/hex/uuid/dotenv) | "See `nfmt`, `ncanon`; `codec` capabilities are being folded in." |
| `collections` | `dsa` maps/sets | "Use `dsa`. `collections` is the IndexMap/hash backing crate." |

See `../MASTER_REPORT.md` §6 for the consolidation rationale.
