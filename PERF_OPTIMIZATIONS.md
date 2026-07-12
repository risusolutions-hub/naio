# Niao performance pass — 2026-07-13

Two-part deliverable: (1) safe speedups already applied to the source, and (2) higher-leverage
changes written as ready-to-apply diffs that need a local `cargo build` + benchmark to confirm.

Everything here targets the four areas you picked: **build profile, startup/load, GC & allocation,
interpreter hot loop.**

> Context: these edits were made without a compiler in the loop (the sandbox has no Rust toolchain
> and your artifacts are Windows binaries). The applied set is limited to changes that are local,
> cannot break dependency resolution, and provably preserve behavior. Build and run the harness on
> Windows to confirm, then apply the Part 2 items one at a time.

---

## Part 1 — Applied (safe, behavior-preserving)

| # | File | Change | Why it's faster |
|---|------|--------|-----------------|
| 1 | `Cargo.toml` | `[profile.release] lto = "thin"` → `"fat"` | Fat LTO inlines across crate boundaries. The hot path spans `niao_vm` → `niao_runtime` → `niao_bignum`; thin LTO can't inline those. Bigger binary + slower link, faster runtime. |
| 2 | `crates/niao_vm/src/lib.rs` (`dispatch_step`) | Wrapped the per-instruction `dsa_loops[func_idx].get(&ip)` probe in `if !…is_empty()` | That hashmap probe ran on **every** bytecode instruction. Functions with no fused DSA loops (the common case) now skip the hash entirely. Identical semantics — an empty map's `get` returns `None` anyway. |
| 3 | `crates/niao_vm/src/gc.rs` | `#[inline]` on `maybe_collect` and `alloc_heap` | `maybe_collect` runs once per instruction; `alloc_heap` on every heap allocation. Inlining the cheap common path avoids a call each time (matters most in non-LTO/debug and reinforces the LTO build). |
| 4 | `crates/niao_vm/src/lib.rs` (`load_module`) | Removed a duplicate `self.field_names = module.field_names.clone()` (it was cloned twice per load) | One fewer full clone of the field-name table on every program start. Pure startup win. |

**Verify Part 1:**
```cmd
cargo build --release
cargo test -p niao_vm
python benchmarks\benchmark_full.py
```
None of these four should change any test result or program output. If a benchmark regresses, the
only plausible culprit is fat LTO on your toolchain — revert #1 alone to isolate.

---

## Part 2 — Ready to apply, verify locally before committing

Ordered by expected payoff. Apply one, build, benchmark, keep or revert.

### 2.1 Flatten the dispatch loop  *(biggest interpreter win)*

Today `dispatch()` calls `dispatch_step()` — which is `#[inline(never)]` — **once per bytecode op**.
Every instruction pays for: a function call, recomputing `frame_top` from `frames.len()`, re-indexing
`self.frames[frame_top]`, and re-reading `self.functions[func_idx].code.len()`. That per-op bookkeeping
is classic interpreter overhead.

**Approach** (do this by hand, it's a refactor, not a paste):
- Move the big `match op { … }` *into* a `loop` inside `dispatch()` (or a new `run_frame`).
- Hoist `func_idx` and a raw pointer/slice to the current function's `code` into locals; only re-fetch
  them when the frame changes (on `Call`, `Return`, `Halt`, frame fall-off).
- Keep `ip` in a local `usize`; write it back to `self.frames[frame_top].ip` only when you leave the
  loop (call/return/jump-to-another-frame/GC). Read it from the local in the hot arithmetic ops.
- Call `maybe_collect()` on **backward jumps and calls only**, not every op — heap only grows at
  allocation sites, and a periodic check at loop backedges bounds it just as well.

Expected: measurable throughput gain on `math_bench` / `dsa_bench` (interpreter-bound loops).
Risk: **high** — 400+ match arms rely on `self.frames[frame_top].ip`. Do it incrementally and lean on
`cargo test -p niao_vm` after each step. This is why it's not pre-applied.

### 2.2 Tune GC thresholds  *(throughput vs. memory)*

`crates/niao_vm/src/gc.rs`:
```rust
pub(crate) const GC_INTERVAL: u32 = 16384;          // -> 32768
pub(crate) const GC_THRESHOLD_INITIAL: usize = 24576; // -> 49152
```
Fewer, larger collections = better throughput, more peak memory. The adaptive logic in `collect()`
(halve on low live-ratio, grow up to 1 MiB slots) still bounds it.

**Must verify:** the test `gc_compacts_unreachable_heap_slots` asserts final heap `< 10_000`. Doubling
`GC_INTERVAL` keeps `100000 mod 32768 == 1696` (same remainder as today), so it *should* still pass —
but the adaptive threshold interacts with it, so run `cargo test -p niao_vm` before committing.

### 2.3 Reuse GC mark buffers  *(cuts allocation in `collect()`)*

Each collection allocates two fresh `vec![false; …]` mark bitmaps. Reuse them across collections.
Results are byte-for-byte identical, so no test/behavior change.

In `struct Vm` add:
```rust
    gc_mark_heap: Vec<bool>,
    gc_mark_native: Vec<bool>,
```
In `Vm::new()` initialize:
```rust
    gc_mark_heap: Vec::new(),
    gc_mark_native: Vec::new(),
```
In `collect()` replace the two `let mut marked_* = vec![false; …];` lines with the take/reuse pattern
(avoids a self-borrow conflict):
```rust
    let mut marked_heap = std::mem::take(&mut self.gc_mark_heap);
    marked_heap.clear();
    marked_heap.resize(self.heap.len(), false);
    let mut marked_native = std::mem::take(&mut self.gc_mark_native);
    marked_native.clear();
    marked_native.resize(self.native_ds.len(), false);
```
Then, after the `compact_vec(… &marked_native)` calls (they're the last readers), hand the buffers back:
```rust
    self.gc_mark_heap = marked_heap;
    self.gc_mark_native = marked_native;
```

### 2.4 Faster global allocator (mimalloc)  *(biggest single allocation win, esp. on Windows)*

The Windows system allocator is comparatively slow; niao is allocation-heavy (arrays, boxed values,
GC). A drop-in global allocator typically buys 5–20% on allocation-bound workloads.

`crates/niao_cli/Cargo.toml`:
```toml
[dependencies]
mimalloc = { version = "0.1", default-features = false }
```
`crates/niao_cli/src/main.rs` (top of file):
```rust
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
```
Trade-off: it adds a dependency, which cuts against your zero-dep trend — but a global allocator is the
usual exception. If you'd rather keep it optional, put it behind a `fast-alloc` cargo feature (default-on).

### 2.5 `panic = "abort"`  *(smaller/faster binary — behavior trade-off)*

```toml
[profile.release]
panic = "abort"
```
Removes unwinding tables/landing pads and can help the optimizer. **But** it changes web-server
isolation: a panic in an async request handler currently unwinds and tokio/axum turn it into a 500;
with `abort` it kills the whole process. You have no `catch_unwind` in your own crates, so internal code
is fine — the only question is whether you want per-request panic isolation in `niao_web`. Apply only if
you're comfortable with a panic taking the server down.

### 2.6 `target-cpu=native` — local benchmarking only

For your own machine (NOT the distributed release artifacts — it breaks portability), add
`.cargo/config.toml`:
```toml
[build]
rustflags = ["-C", "target-cpu=native"]
```
Lets the VM's integer/float ops and `niao_bignum` use your CPU's full instruction set. Keep this out of
the release/CI config that produces the Windows x64/x86/ARM64 bundles.

---

## Measuring

Baseline first, then after each change:
```cmd
cargo build --release
python benchmarks\benchmark_full.py
niao run benchmarks\math_bench_heavy.niao --time
niao run benchmarks\dsa_bench.niao --time
```
Your `benchmarks\baseline.json` + harness already track this — compare against it so you keep only the
changes that actually move the numbers on your hardware.
