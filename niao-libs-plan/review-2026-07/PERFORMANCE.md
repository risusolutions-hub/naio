# Niao stdlib — Performance analysis (v0.2.3 → v0.2.4)

Code-grounded optimization notes. Each item cites the file/loop it came from. "N×" figures are
**engineering estimates from reading the code** (FLOP/alloc/vector-width counting), not measured —
regenerate real numbers with `harness/run_bench.py` on Windows.

Baseline to beat (measured, v0.2.2): **345M ops/s arithmetic, 14.3 MB RSS, `fib(40)` 0.08 ms.**

---

## 0. The value model sets the ceiling

`ValueRef = Rc<RefCell<Value>>`. Every scalar touched individually pays:
- an `Rc` refcount bump on clone/drop,
- a `RefCell` borrow-flag check on access.

The runtime already bypasses this with **packed columns** (`FloatArray(Vec<f64>)`, `IntArray(Vec<i64>)`).
Rule of thumb for every numeric lib: **stay in packed space end-to-end; never round-trip through
`Vec<ValueRef>` in a loop.** Most wins below are corollaries of this.

---

## 1. `nvec` — vector search (flagship, biggest single win)

**File:** `crates/niao_runtime/src/nvec/index.rs`

### 1a. Norms are recomputed every comparison
```rust
// index.rs:84  (current)
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot=0.0; let mut na=0.0; let mut nb=0.0;
    for (&ai,&bi) in a.iter().zip(b.iter()) { dot+=ai*bi; na+=ai*ai; nb+=bi*bi; }
    let denom = na.sqrt()*nb.sqrt();
    if denom < 1e-10 {0.0} else {dot/denom}
}
```
A query over N stored vectors recomputes **the query norm N times** and **each stored norm on every
call**. That is 3 mul-adds per element when 1 is enough.

**Fix:** store `inv_norm = 1.0 / ‖v‖` on each `VecEntry` at insert; L2-normalize the query once per
search. Then per comparison it's a **pure dot product** and the score is `dot * q_inv * e_inv`.
~3× fewer FLOPs on the inner loop, exact same results.

### 1b. Scalar `f32`, no SIMD
The dot loop is scalar. An 8-wide `f32` accumulation (`std::simd::f32x8` or manual unroll with 4
independent accumulators to break the dependency chain) vectorizes cleanly.
**Combined with 1a: estimate 2–3× end-to-end on brute-force search (N ≤ 256), and faster NSW hops.**

### 1c. Structural
- Single-layer NSW (`NSW_M=16`, `ef=64`). For N ≫ 10⁵, a **true multi-layer HNSW** cuts hops.
- `SearchHit` clones `metadata: HashMap` per hit (44 `.clone()` in `nvec/mod.rs`) — return `Rc`/index
  references and hydrate metadata only for the final top-k.
- Add **int8 / binary quantization** option: 4× less memory, faster dot, ~1% recall loss. (lightweight + speed)

---

## 2. `nsimd` — the shared numeric kernel

**File:** `crates/niao_runtime/src/nsimd.rs` (`UNROLL = 8`, `chunks_exact(8)`)

Today it is *autovectorized scalar*: correctness-portable, but the compiler decides whether SIMD
happens and it usually under-vectorizes reductions (dependency chains).

**Fix:** a small portable kernel layer using `std::simd` (portable-SIMD is stable-adjacent; or
`core::arch` x86_64 `avx2`/`fma` behind `is_x86_feature_detected!` with a scalar fallback):
- `add/mul/sub/div`, `sum`, `dot`, `min/max`, `scale`, `axpy` — all 8-wide.
- Break reduction dependency chains with 4 accumulators.
- Because `nsimd` backs `ncl`, `nml`, `npar`, `nlazy`, one kernel upgrade lifts all of them.

**Estimate:** 2–4× on elementwise + reductions vs. the current unrolled-scalar loops.

---

## 3. `ncl` — dataframe engine (3,570 LOC)

**Dir:** `crates/niao_runtime/src/ncl/`

- **Capacity hints:** `groupby`, `join`, and `io` build result columns with `push` (11 in `ncl/io.rs`).
  Reserve to known/estimated size — groupby result rows ≤ distinct keys; join ≤ left×selectivity.
- **Fused agg:** compute `sum`/`count`/`mean` in one pass over a group rather than one pass per agg.
- **Zero-copy filters:** boolean masks should produce **index vectors** applied lazily, not
  materialize a new column per filter (feeds `nlazy`).
- **Hash-join:** ensure the build side hashes the *smaller* frame; probe with the packed key column,
  not per-row `ValueRef`.
- **Parallel groupby:** partition by key-hash across `npar` threads for large frames.

**Estimate:** 10–30% on typical groupby/join pipelines; more when filters chain.

---

## 4. `json` — parse/stringify (1,233 LOC, 43 push-sites)

**File:** `crates/niao_runtime/src/json.rs`

- **Reserve the output `String`** in `stringify` proportional to input size (a good heuristic is
  `input_len` for reparse, or `2× value count`); avoids repeated realloc growth.
- **SIMD string/whitespace scan** in the parser: skip whitespace and find the next `"`/`\`/control
  byte 16/32 bytes at a time (classic simdjson-lite). Big win on whitespace-heavy documents.
- **Number fast-path:** integers without `.`/`e` parse via a tight `i64` accumulator, bypass the
  float path.
- **String fast-path:** when a string span has no escapes, copy the slice wholesale instead of
  char-by-char.

**Estimate:** 15–40% on large-document parse; 10–20% on stringify from the reserve alone.

---

## 5. `nrand` — PRNG (xoshiro256\*\*) — **already well-optimized; feature-only wins**

**File:** `crates/niao_runtime/src/nrand.rs`

> **Correction after reading the source:** the scalar core is *already* optimal. `next_below`
> (line 83) already uses **Lemire's nearly-divisionless** bounded generation
> (`(x as u128 * bound) >> 64` with the rare-rejection branch), and `next_normal` (line 107) already
> **caches the spare normal** (`self.spare_normal`) so Box-Muller yields two normals per uniform pair.
> Do **not** "fix" these — they're correct and fast.

Remaining real wins are about **packed generation**, not the scalar core:
- Add **`fill(FloatArray)` / `fill_int(IntArray)`** to generate directly into a packed column, skipping
  per-element `ValueRef` allocation — the only place `nrand` currently leaves speed on the table when
  feeding `ncl`/`nml`.
- `bytes` (line 302) already batches 8 bytes per `next_u64`; fill the final partial word with one
  `copy_from_slice` of the low bytes rather than a byte loop (tiny).
- Everything else here is **features** (poisson/binomial/gamma/beta, `uuid4`, `permutation`), not speed.

**Estimate:** packed `fill` removes N `ValueRef` allocations on bulk generation; scalar throughput is
already at its ceiling.

---

## 6. `nstr` — string toolkit (1,023 LOC, 55 methods)

**File:** `crates/niao_runtime/src/nstr.rs`

- `char_at`/`char_len`/`chars` iterate codepoints — fine, but document that they are **codepoint**,
  not **grapheme**, aware. Add grapheme variants for user-facing width/truncation.
- `levenshtein`/`similarity` allocate two full rows — already O(min) is possible with a single rolled
  row + early-exit band for a threshold. Add `levenshtein_within(a,b,max)` that bails past `max`.
- `replace`/`replace_n`/`split` — reserve output using `count(needle)` when cheap.
- Add a **`format(template, args)`/`sprintf`** builtin (see roadmap) — most-requested gap.

---

## 7. Allocation discipline (mechanical, low-risk, broad)

Add `with_capacity`/`String::with_capacity` at these push-heavy sites:

| Lib | File | Push-sites | Reserve to |
|---|---|---|---|
| json | `json.rs` | 43 | input length (parse) / value count (stringify) |
| ntoml | `ntoml.rs` | 27 | table/line count |
| ncanon | `ncanon.rs` | 25 | value count |
| dsa | `dsa.rs` | 25 | known n |
| nurl | `nurl.rs` | 22 | input length |
| nsnap | `nsnap.rs` | 18 | value count |
| ncsv | `ncsv.rs` | 18 | rows × cols |
| nlog | `nlog.rs` | 17 | field count |

**Estimate:** 10–30% on the affected serialize/parse paths; near-zero risk.

---

## 8. `.clone()` reduction (latency + memory)

Highest-count offenders where borrows/`Rc::clone` handles suffice: `nmem` (45), `nvec` (44),
`nmongo` (65 across `crud.rs`+`types.rs`), `ndebug` (27), `nargs` (25), `nagent` (25),
`nworkspace`/`nsketch`/`nconfig` (22). Pattern: pass `&Value`/`&str` into helpers; clone only at the
final store. In request builders (`nmongo`, `naws`, `nazure`, `nsupa`) build the wire buffer with a
single reserved `String`/`Vec<u8>` instead of concatenating cloned fragments.

---

## 9. `#[inline]` on cross-module hot helpers

Only ~70 `#[inline]` in the runtime. Add to: `nsimd` element ops, `nvec::cosine` (already inline) +
its dot helper, `nrand::next_u64`/`next_f64`, `ncl` column accessors, `nmath` scalar wrappers used in
loops, `ncanon`/`json` byte classifiers. 5–15% on the kernels that cross a crate boundary.

---

## 10. Lightweight / binary-size

- **Consolidate the 11 legacy crate-libs** (`archive, args, bignum, codec, collections, crypto, http,
  io, log, net_clients, rand`) into their native twins — removes whole modules from the 41 MB binary.
- **Feature-gate heavy optional stacks** (`nllm`/llama.cpp, `nrag`/onnx, `nsqlite` C amalgamation,
  tls) behind cargo features so a minimal `niao` build drops them. The CI already excludes
  `niao_llm`/`niao_rag` — formalize that as `--no-default-features`.
- `strip = true` + `opt-level = "z"` on a `min` profile for distribution builds; `panic = "abort"`.
- **Lazy-init big static tables** (tokenizer vocab, tz database in `time`, color tables in `ncolor`)
  with `OnceLock` so they cost nothing until used.

---

## How to get measured numbers

```powershell
# on Windows, from repo root
python benchmarks\benchmark_full.py                # regenerates the language comparison table
python niao-libs-plan\review-2026-07\harness\run_bench.py   # per-lib micro-benchmarks (this review)
```
The harness writes `harness/results/<date>.json` so you can diff v0.2.3 → v0.2.4 after each change.
Land one optimization at a time and re-run — that keeps every "N×" honest.
