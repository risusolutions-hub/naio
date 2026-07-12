# Niao stdlib — per-library v0.2.4 roadmap

For every one of the 112 libs: **Add** (new features), **Change** (API/behavior tweaks), **Speed**
(perf/lightweight). Flagship entries cite specific code; smaller entries are domain-standard gaps —
verify against current source before implementing. Estimates are engineering judgments, not measured.

**Legend:** 🟢 low-risk · 🟡 real engineering · 🔴 large/risky · ⚡ speed · ✨ feature · 🔧 change

---

## A. Language core & runtime

### core (17 builtins) — `lib.rs`
- ✨ Add `dbg(x)` (prints + returns value), `todo()`/`unreachable()`, `identity`, `clamp` for ints.
- 🔧 Split the giant `lib.rs` (1,708 LOC) registration into `builtins/*.rs` — maintainability.
- ⚡ Ensure `len`/`type`/array index are `#[inline]` and avoid a `RefCell` borrow when the VM already holds the value.
- 📄 **No doc** — write `docs/CORE.md` (`docs-proposed/CORE.md` drafted).

### dsa (90 builtins, 2,248 LOC) — `dsa.rs`, `dsa_storage.rs`
- ✨ Add `binary_search`, `partial_sort`/`top_k`, `heap_merge`, `union_find`, `lru_ordered_map`, priority-queue `decrease_key`.
- ✨ Graph: Dijkstra/A\* with a real binary heap, topological sort, SCC (Tarjan), BFS/DFS iterators.
- ⚡ 25 push-sites without `with_capacity` — reserve when `n` is known (sorts, graph adjacency).
- 🔧 Document complexity per op in the doc table (users pick structures by Big-O).

### collections (0 builtins exposed, 1,731 LOC) — crate `niao_collections`
- 🔧 **Overlaps `dsa` maps/sets.** Decide canonical: keep `collections` as the Rust-level IndexMap/hash backing, expose only through `dsa`. Alias or hide the standalone package.
- ⚡ Confirm `ahash`-equivalent hasher is used for int keys; add a fast-path `IntMap` (identity hash) for `i64` keys.

### bignum (8 builtins, 1,079 LOC) — crate `niao_bignum`
- ✨ Add modular exponentiation `mod_pow`, `gcd`/`lcm` on bigints, `is_probable_prime` (Miller-Rabin), bit ops, `from_bytes`/`to_bytes`.
- ⚡ Karatsuba threshold tuning for multiply; reuse limb buffers across ops.
- 📄 **No doc** — draft in `docs-proposed/BIGNUM.md`.

### json (15 builtins, 1,233 LOC) — `json.rs`
- ✨ Add streaming/`json_parse_file`, `json_pointer(doc, "/a/b/0")`, `merge`, `diff`, pretty-print width control.
- ⚡ Reserve output buffer; SIMD whitespace/string scan; int and no-escape-string fast paths (see PERFORMANCE §4). **15–40%.**
- 🔧 Surface parse errors with line/column + byte offset.

### nstr (55 builtins, 1,023 LOC) — `nstr.rs`
- ✨ Add **`format`/`sprintf`** (most-requested gap), `find_all`, `split_lines(keepends)`, Unicode **NFC/NFD** normalize, grapheme-aware `width`/`truncate`, `strip_ansi`.
- 🔧 Document codepoint-vs-grapheme semantics of `char_*`.
- ⚡ Reserve outputs in `replace`/`split`; add `levenshtein_within(a,b,max)` banded early-exit. (PERFORMANCE §6)

### nmath (48 builtins, 768 LOC) — `nmath.rs`
- ✨ Add `erf`/`erfc`, `gamma`/`lgamma`, `beta`, `fma`, `copysign`, `nextafter`, `cumsum`/`cumprod`, `corr`/`covariance`, `histogram`, `quantile(method)`, `softmax`, `logsumexp`.
- ⚡ Provide **vectorized** counterparts over `FloatArray` (delegate to `nsimd`) so stats on columns skip per-element `ValueRef`.
- 🔧 `percentile`/`median` — expose interpolation mode (linear/nearest/lower).

### nfmt (14 builtins, 555 LOC) — `nfmt.rs`
- ✨ Add `table` (aligned columns), `relative_time` ("3m ago"), `plural`, `truncate_middle`, locale-aware grouping.
- ⚡ Reserve the template output `String`; precompile the `{}` template into a token vec once (cache by template string).

### nrand (20 builtins, 758 LOC) — `nrand.rs`
- ✨ Add `poisson`, `binomial`, `gamma`, `beta`, `uuid4`, `permutation`, `fill(FloatArray)`, per-call seeded generator handle everywhere.
- ⚡ Core PRNG is **already optimal** — Lemire bounded ints + Box-Muller spare-cache are in the source (verified). Real win is `fill(FloatArray)` packed generation to skip per-element `ValueRef` alloc. (PERFORMANCE §5)

### rand (9 builtins, 492 LOC) — crate `niao_rand`
- 🔧 **Legacy twin of `nrand`.** Consolidate: make `rand` an alias, port any unique surface into `nrand`, retire the crate path for callers.

### time (32 builtins, 770 LOC) — `time.rs`
- ✨ Add `parse` with format tokens + RFC3339/RFC2822, `duration` arithmetic type, `now_monotonic`, business-day math, `format_relative`.
- ⚡ Lazy-init the tz table via `OnceLock` (lightweight until first zoned call).
- 🐞 Manifest was NUL-corrupted (fixed in `manifest-fixes/`).

### re (22 builtins, 589 LOC) — `re.rs`
- ✨ Add named capture groups, `find_iter` streaming, `replace_fn` (callback), compiled-pattern handle cache, anchored/multiline flags surfaced.
- ⚡ Cache compiled programs by pattern string (avoid recompiling in loops); add a literal-prefix fast reject.
- 🔴 If backtracking today, consider a lazy-DFA/Thompson-NFA path for linear-time guarantees (large; keep old engine until green).

### io (builtin count unknown — manifest truncated) — `io.rs` (1,344 LOC)
- 🐞 **Fix + fill `builtin_count`** (see MASTER_REPORT §5).
- ✨ Add buffered readers/writers with configurable capacity, `read_lines` iterator, `copy(src,dst)` with a reused buffer, temp-file helpers, atomic write (`write_temp` + rename).
- ⚡ Default to 64 KiB buffers; reuse a thread-local scratch buffer for line reads.
- 📄 **No doc** — draft in `docs-proposed/IO.md`.

### nos (23 builtins, 534 LOC) — `nos.rs`
- ✨ Add `spawn` with env/cwd, `which`, `hostname`, `cpu_count`, `home_dir`, signal handling, `glob`.
- 📄 **No doc** — draft in `docs-proposed/NOS.md`.

### nenv (26 builtins, 1,013 LOC) — `nenv.rs`
- ✨ Add typed `require(name)` (error if missing), `.env` interpolation `${VAR}`, secret masking in dumps, profile layering (`.env.local`).
- 🐞 Manifest NUL-corrupted (fixed).

### narena (6 builtins, 375 LOC) — `narena.rs`
- ✨ Add scoped arenas (`with_arena(fn)` auto-reset), stats (bytes reused vs. allocated), typed pools.
- ⚡ This *is* the GC-pressure tool — expose it from `ncl`/`nml`/`json` hot paths so they borrow scratch buffers from a shared arena.

### ncanon (4 builtins, 436 LOC) — `ncanon.rs`
- ✨ Add RFC 8785 JCS canonical JSON mode; stable float formatting; `fingerprint` with pluggable hash (FNV/xxhash/SHA-256).
- ⚡ Reserve buffer (25 push-sites); FNV-1a is fine but add xxhash for large blobs.

---

## B. Numeric & data

### ncl — Niao Column Library (62 builtins, 3,570 LOC) — `ncl/`
- ✨ Add `pivot`/`melt`, window functions (rolling mean/sum, `shift`, `rank`), `merge_asof`, categorical dtype, `read_parquet`/`read_csv` streaming, null-aware aggregations.
- 🔧 Expose lazy frames (build a plan, execute once) — pairs with `nlazy`.
- ⚡ Capacity hints on groupby/join/io; fused single-pass aggregations; index-vector filters; parallel groupby via `npar`. (PERFORMANCE §3) **10–30%.**
- 🐞 Manifest description mojibake fixed.

### nml — Niao Machine Learning (67 builtins, 1,969 LOC) — `nml/`
- ✨ Add optimizers (AdamW, RMSProp), layers (LayerNorm, Dropout, Conv1d), `save`/`load` weights, autograd tape introspection, mixed-precision (f16 accumulate f32).
- ⚡ Route all tensor kernels through the upgraded `nsimd`/`npar`; blocked GEMM with cache tiling; avoid per-op tensor clone.
- 🔴 GPU path: keep FFI, but ensure host↔device copies are batched.
- 🐞 Manifest mojibake fixed.

### nvec — vector database (10 builtins, 2,120 LOC) — `nvec/`
- ✨ Add int8/binary **quantization**, metadata filtering during search, batch upsert/query, `save`/`load` mmap-backed index, cosine+dot+L2 metrics.
- ⚡ **Precompute norms + SIMD dot + top-k without metadata clone** — the single biggest win. (PERFORMANCE §1) **2–3×.**
- 🔴 True multi-layer HNSW for N ≫ 10⁵.

### nsimd (9 builtins, 607 LOC) — `nsimd.rs`
- ✨ Add `dot`, `axpy`, `min/max/argmin/argmax`, `prefix_sum`, `clamp`, i32/f32 kernels (not just f64/i64).
- ⚡ **Port to `std::simd` / `core::arch` with scalar fallback + 4-accumulator reductions.** Lifts ncl/nml/npar/nlazy. (PERFORMANCE §2) **2–4×.**
- 🔧 Runtime CPU feature detection (`is_x86_feature_detected!`) to pick AVX2/NEON/scalar.

### npar (7 builtins, 454 LOC) — `npar/`
- ✨ Add `par_map_reduce`, `par_sort`, `par_groupby`, chunked parallel over `FloatArray`, `set_threads` persistence.
- ⚡ Tune the work-stealing chunk size by element count; avoid spawning for tiny inputs (below a threshold run serial).

### parallel (38 builtins, 1,592 LOC) — `parallel/`
- 🔧 **Distinct from `npar`:** this is threads/mutexes/channels/worker-pools; `npar` is data-parallel ops. Keep both but cross-link in docs so users pick the right one.
- ✨ Add scoped threads, `select!` over channels, bounded channels with backpressure, `Once`/barrier, atomics, structured task groups with cancellation.
- ⚡ Reuse the worker pool across calls (avoid re-spawn); park idle workers instead of busy-poll.
- 🐞 Manifest NUL-corrupted (fixed).

### nlazy (9 builtins, 633 LOC) — `nlazy.rs`
- ✨ Add `flat_map`, `zip`, `enumerate`, `scan`, `chunk`, `window`, early-terminating `find`/`any`/`all`.
- ⚡ Fuse map→filter→take into one loop (already the design) — ensure no intermediate `Vec` materializes between stages; SIMD the map stage when the closure is a known numeric op.

### nsoa (8 builtins, 573 LOC) — `nsoa.rs`
- ✨ Add typed column push, `sort_by_column`, `filter_mask`, `to_ncl`/`from_ncl` bridge, row iterator.
- ⚡ Struct-of-arrays is already cache-friendly; add SIMD reductions per column via `nsimd`.

### ncolumnar (5 builtins, 644 LOC) — `ncolumnar.rs`
- ✨ Add per-column compression (RLE, delta, dictionary), column projection on read, schema evolution.
- ⚡ Reserve on encode; memory-map on decode via `nmmap` for zero-copy columns.

### nmmap (9 builtins, 571 LOC) — `nmmap.rs`
- ✨ Add `search_all`, regex search over mapped bytes, writable maps with flush, madvise hints (sequential/random).
- ⚡ Cache the lazy line index; `memchr`-style SIMD byte search instead of scalar scan.

### nsketch (12 builtins, 771 LOC) — `nsketch.rs`
- ✨ Add t-digest (quantiles), MinHash/LSH (similarity), full HyperLogLog++ (currently "lite"), Cuckoo filter.
- ⚡ 22 clones — hash once, reuse; pack registers into a `Vec<u8>`; SIMD the HLL harmonic-mean pass.

### nshape (5 builtins, 496 LOC) — `nshape.rs`
- ✨ Add broadcast-compatibility check, dtype inference, `assert_shape(x, [.., 3])` with wildcards.
- 🔧 Share the shape model with `nml`/`ncl` so shapes are one type across libs.

### npersist (13 builtins, 550 LOC) — `npersist.rs`
- ✨ Add persistent `Set`, `OrderedMap`, transient (mutable) builders for batch construction then freeze.
- ⚡ Structural sharing is there; add a transient path so bulk inserts don't clone the trie per step.

---

## C. AI / LLM toolkit

### nllm — GGUF inference (13 builtins, 768 LOC) — `nllm/`
- ✨ Add streaming token callbacks with stop sequences, JSON/grammar-constrained decoding, batched prompts, KV-cache reuse across turns, logprobs.
- 🔴 Keep llama.cpp FFI (per MASTER_PLAN); expose n_threads/n_gpu_layers/mmap flags.
- 📄 **No doc** — draft in `docs-proposed/NLLM.md`.

### nrag — vector RAG (15 builtins, 663 LOC) — `nrag/`
- ✨ Add chunk overlap strategies, hybrid search (BM25 + vector), reranking hook, metadata filters, incremental index update.
- ⚡ Batch-embed then bulk-insert; reuse `nvec`'s precomputed norms (PERFORMANCE §1) for the cosine stage.
- 📄 **No doc** — draft in `docs-proposed/NRAG.md`.

### nembed (13 builtins, 584 LOC) — `nembed.rs`
- ✨ Add pluggable embedder backends (local hash / nllm / remote), batch API, dimension reduction (PCA), on-disk cache.
- ⚡ Content-hash cache is good; make the local embedder SIMD and cache norms alongside vectors.

### ntok — BPE tokenizer (9 builtins, 1,529 LOC) — `ntok.rs`
- ✨ Add special-token handling, batch `encode`, vocab export/import, byte-fallback, regex pretokenizer presets (GPT-2/Llama).
- ⚡ Cache the merge-rank map as a `HashMap<(u32,u32),u32>`; 19 clones — intern tokens as `u32` ids and avoid `String` clones in the merge loop.

### nctx (6 builtins, 486 LOC) — `nctx.rs`
- ✨ Add real tokenizer-backed counting (delegate to `ntok`), per-model context windows, sliding-window + summary trim strategy, message-role budgeting.
- 🔧 Today counts are estimates — mark estimate vs. exact.

### neval (15 builtins, 852 LOC) — `neval.rs`
- ✨ Add BLEU/ROUGE/exact-match/F1, pass@k, calibration curves, cost-per-eval, dataset sampling, HTML report export.
- ⚡ Run dataset rows through `npar` for parallel eval.

### nprompt (4 builtins, 547 LOC) — `nprompt.rs`
- ✨ Add validated choices, password/hidden input, multi-select, default timeouts, non-TTY JSON mode.
- 🔧 Already has pipe fallback — add `--yes` / env override for CI.

### nprovider (10 builtins, 678 LOC) — `nprovider.rs`
- ✨ Add streaming-aware failover, per-provider rate-limit awareness (pair with `nquota`), retry with backoff+jitter, cost-aware routing (pair with `ncost`), live pricing refresh.
- 🔧 Keep the pricing table in a data file, not code, so updates don't need a rebuild.

### nagent (12 builtins, 625 LOC) — `nagent.rs`
- ✨ Add tool-call scheduling, dependency graph between agents, shared blackboard memory (pair with `nmem`), cancellation/timeout per step, run tracing (pair with `ntrace`).
- ⚡ 25 clones — pass agent context by handle/`Rc`, not by value.

### ncost (6 builtins, 652 LOC) — `ncost.rs`
- ✨ Add running-total ledger, budget alerts (pair with `nbudget`), multi-currency, per-request attribution, export CSV.
- 🔧 Share the pricing table with `nprovider` (one source of truth).

### nbudget (9 builtins, 564 LOC) — `nbudget.rs`
- ✨ Add hierarchical budgets (global → task → step), soft/hard limits, callbacks on threshold, persistence across runs.
- 🔧 Unify the "resource limit" concept across `nbudget`/`nquota`/`npace`/`ncap` — currently four overlapping models.

### nbatch (5 builtins, 517 LOC) — `nbatch.rs`
- ✨ Add dynamic batching by latency target, padding strategies, OOM backoff (halve on failure), throughput autotune.
- ⚡ Read VRAM/RAM once per window (pair with `ngpu`/`nram`), not per suggestion.

### nschema (6 builtins, 689 LOC) — `nschema.rs`
- ✨ Add JSON-Schema draft 2020-12 subset, `$ref`, enum/format keywords, LLM tool-schema export, TypeScript type gen.
- ⚡ Compile schema to a validator closure once; reuse across records.

### ntemplate (8 builtins, 438 LOC) — `ntemplate.rs`
- ✨ Add partials/includes, conditionals/loops, template registry with semver (pair with `nsemver`), token-budget-aware rendering.
- ⚡ Precompile templates to a token vec; cache by (name,version).

### nguard (8 builtins, 661 LOC) — `nguard.rs`
- ✨ Add configurable PII types (SSN/CC/phone/email/IP) with Luhn check, allowlist, redaction styles (mask/hash/drop), streaming scan.
- ⚡ Compile denylist to Aho-Corasick for single-pass multi-pattern scan instead of N passes.

---

## D. Web, networking & databases

### net (55 builtins, 2,085 LOC) — `net/`
- ✨ Add HTTP/2 client, connection pooling + keep-alive, retry/backoff, multipart upload, streaming response bodies, timeouts per phase.
- ⚡ Reuse read buffers across requests; pool TCP connections by host; avoid `String` header re-parse.
- 🐞 Manifest NUL-corrupted (fixed).

### http (crate `niao_http`, 2,715 LOC) — HTTP types
- 🔧 **Overlaps `net`.** Keep `http` as the shared type layer (Method/Status/HeaderMap/Uri), have `net` depend on it. Don't ship two HeaderMap impls.
- ✨ Add typed header accessors, cookie jar, URL builder shared with `nurl`.
- 📄 **No doc** — draft in `docs-proposed/HTTP.md`.

### nurl (7 builtins, 945 LOC) — `nurl.rs`
- ✨ Add IDNA/punycode, relative resolution edge cases, query-param ordered multimap, `normalize`.
- ⚡ Reserve output (22 push-sites); percent-decode with a 256-entry lookup table.

### nws (4 builtins, 230 LOC) — `nws.rs`
- ✨ Add auto-reconnect with backoff, ping/pong keepalive, message queue, subprotocol negotiation, binary frames.
- 🔧 Thin wrapper over `net` websocket — expose backpressure.

### nsmtp (2 builtins, 446 LOC) / net_clients (8 builtins, 730 LOC)
- ✨ Add attachments (MIME multipart), HTML+text alt, STARTTLS, connection reuse, BCC/CC, DKIM signing.
- 🔧 `nsmtp` and `net_clients` overlap — `nsmtp` should be the ergonomic front for `net_clients`' SMTP.
- 📄 `net_clients` has **no doc** — draft in `docs-proposed/NET_CLIENTS.md`.

### npg — PostgreSQL (52 builtins, 2,771 LOC) — `npg/`
- ✨ Add `COPY` (bulk load), `LISTEN/NOTIFY`, array/JSONB/UUID type mapping, cursor streaming, prepared-statement cache, connection health checks.
- ⚡ Reuse the wire read/write buffer per connection; batch pipeline mode; binary result format for numeric columns.
- 🐞 Manifest NUL-corrupted (fixed).

### nsqlite — SQLite (39 builtins, 1,879 LOC) — `nsqlite/`
- ✨ Add WAL mode toggle, `backup`/`vacuum`, user-defined functions, `blob` streaming, `EXPLAIN QUERY PLAN` helper.
- ⚡ Prepared-statement cache keyed by SQL; reuse column-value buffers; `PRAGMA` tuning presets.
- 🐞 Manifest NUL-corrupted (fixed).

### nmongo — MongoDB (45 builtins, 3,894 LOC) — `nmongo/`
- ✨ Add aggregation-pipeline builder, bulk write, change-stream resume tokens, GridFS streaming, retryable writes.
- ⚡ **65 clones across `crud.rs`+`types.rs`** — build BSON with a single reserved buffer; borrow filter docs; avoid re-serializing on retry.
- 🐞 Manifest NUL-corrupted (fixed).

### nredis (15 builtins, 752 LOC) — `nredis/`
- ✨ Add pipelining, pub/sub, Lua `EVAL`, connection pool, cluster slot routing, `SCAN` iterator, TTL helpers.
- ⚡ RESP parse into a reused buffer; batch mget/mset already present — extend to pipeline.

### naws (11 builtins, 1,617 LOC) — `naws/`
- ✨ Add S3 multipart upload, presigned URLs, pagination iterators, STS assume-role, SQS/SNS, retry with jittered backoff.
- ⚡ Reuse the SigV4 signing scratch buffers; cache the signing key per (date,region,service).

### nazure (9 builtins, 1,707 LOC) — `nazure/`
- ✨ Add blob block upload, SAS token generation, Table query continuation, Queue Storage, managed-identity auth.
- ⚡ Reuse auth token until expiry; shared HTTP client with `net` pooling.

### nsupa — Supabase (23 builtins, 1,245 LOC) — `nsupa/`
- ✨ Add Realtime (websocket) subscriptions, RPC calls, storage resumable upload, RLS-aware auth refresh, `upsert` conflict targets.
- ⚡ Share one pooled HTTP client; reuse the PostgREST query-string builder buffer.

### nmodel — ORM (10 builtins, 1,595 LOC) — `nmodel/`
- ✨ Add relations (has-many/belongs-to) with eager load, transactions, `find_or_create`, soft delete, query builder `where`/`order`/`limit`, connection to both npg & nsqlite.
- ⚡ Cache generated SQL per (model, op); batch inserts.

### nmigrate (4 builtins, 814 LOC) — `nmigrate.rs`
- ✨ Add down-migrations, checksum drift detection, dry-run diff, multi-dialect (sqlite/pg) codegen, migration status table.
- 🔧 Share the schema model with `nmodel` (one struct DSL).

### nscaffold (5 builtins, 472 LOC) — `nscaffold.rs`
- ✨ Add REST+CRUD generator with validation (`nvalid`), OpenAPI spec output, test scaffolding (`ntest`), config templates.
- 🔧 Keep templates external (data files) so users can customize.

### ahiru — web server (36 builtins, 1,943 LOC) — `ahiru/`
- ✨ Add route groups with middleware chains (partly present), WebSocket routes, static-file serving with range + etag, request-body limits, graceful shutdown, per-route rate limit (pair `nquota`).
- ⚡ Pool VM instances (already `vm_pool`) — ensure zero-alloc happy-path for small JSON responses; reuse response buffers.

---

## E. Observability, DX & tooling

### nlog (11 builtins, 582 LOC) — `nlog.rs`
- ✨ Add log rotation, sampling, per-module levels, span context propagation (pair `ntrace`), async non-blocking writer.
- ⚡ Reserve the line buffer (17 push-sites); skip formatting entirely when a level is filtered out (guard before build).

### log (crate `niao_log`, 1,018 LOC)
- 🔧 **Legacy twin of `nlog`.** Consolidate: `nlog` is the native front; keep `niao_log` as the Rust backend only. Alias the package.

### ntrace (8 builtins, 582 LOC) — `ntrace.rs`
- ✨ Add OTLP export, span links, baggage, sampling policies, flamegraph output.
- ⚡ 18 clones — store spans in an arena; avoid cloning attributes on export.

### nprofile (8 builtins, 551 LOC) — `nprofile.rs`
- ✨ Add flamegraph export, allocation counters, per-call histograms, `profile(fn)` decorator.
- ⚡ Use monotonic clock; keep sample vectors preallocated per span.

### nbench (5 builtins, 528 LOC) — `nbench.rs`
- ✨ Add throughput mode, outlier rejection, regression compare vs. saved baseline (this review's harness), Markdown/JSON report.
- 🔧 Standardize output so `harness/` and `nbench` share one result schema.

### ntest (14 builtins, 488 LOC) — `ntest.rs`
- ✨ Add parameterized cases, `setup`/`teardown`, tags/filters, snapshot assertions (pair `nsnap`), parallel run, JUnit XML output.
- 🔧 First-class `assert_raises`/error-code assertions.

### nlint (3 builtins, 1,013 LOC) — `nlint.rs`
- ✨ Add autofix suggestions, rule severity config, inline `// nlint:allow`, more built-in rules (unused, shadow, dead branch).
- ⚡ 19 clones — walk AST by reference; cache parsed rule set.

### ndoc (3 builtins, 518 LOC) — `ndoc.rs`
- ✨ Add HTML/Markdown site generation from doc comments, cross-links, example extraction to `examples/`.
- 🔧 Feed `docs/` generation from source so docs never drift (addresses the 15 missing docs structurally).

### ndebug (12 builtins, 718 LOC) — `ndebug.rs`
- ✨ Add conditional breakpoints, watch expressions, step-back over checkpoints, diff-to-previous view.
- ⚡ 27 clones — checkpoints should structural-share via `npersist` instead of deep-cloning values.

### ncrash (6 builtins, 431 LOC) — `ncrash.rs`
- ✨ Add symbolized backtraces, breadcrumb trail, redaction (pair `nguard`), export to file + optional webhook.

### nfuzz (9 builtins, 494 LOC) / nreplay (11) / ncassette (12)
- ✨ `nfuzz`: add shrinking, coverage-guided generation, typed generators (struct/array). ✨ `nreplay`/`ncassette`: matchers by header/body, TTL, redaction, streaming bodies.
- ⚡ `ncassette` 19 clones, `nreplay` 21 clones — store recorded frames once, replay by reference.

### nwatch (9 builtins, 429 LOC) / nhotreload (9, 590 LOC)
- ✨ `nwatch`: debounce, recursive globs, event coalescing. ✨ `nhotreload`: state-preserving reload, reload hooks, error isolation.
- ⚡ `nhotreload` 20 clones — diff function bodies by hash before deep-comparing AST.

### nrepl (7 builtins, 621 LOC) — `nrepl.rs`
- ✨ Add multiline input, history, tab-completion, `:load`/`:reset`, pretty value printing.

### nsnap (9 builtins, 631 LOC) / ndiff (3, 527 LOC) / nwhy (9, 506 LOC)
- ✨ `nsnap`: inline snapshot update mode, redaction. ✨ `ndiff`: patch output + apply, myers diff for arrays. ✨ `nwhy`: graph export (dot), provenance query.
- ⚡ Share one deep-equal/deep-diff core across `nsnap`/`ndiff`/`ncontract`/`ncanon` (currently duplicated).

### nexplain (5) / ncontract (5) / ncap (8) / nquota (7) / npace (7) / nfallback (8)
- ✨ `nexplain`: rule packs per error family, links to docs. `ncontract`: `@invariant` decorators, class contracts. `ncap`: fs/net/env scoped capabilities, audit log. `nquota`: distributed token bucket, per-key limits. `npace`: PID-style controller. `nfallback`: half-open circuit state, health probes.
- 🔧 **Unify** `nbudget`/`nquota`/`npace`/`ncap` under a shared "resource governor" model (four overlapping today).

### nsemver (5) / nworkspace (7) / nconfig (9)
- ✨ `nsemver`: prerelease/build-metadata ordering, caret/tilde ranges, `satisfies`. `nworkspace`: parallel member builds, dependency-aware topo run, `--changed` since git ref. `nconfig`: schema migration, secret providers, hot-reload on file change, `--set key=val` override.
- ⚡ `nconfig` 22 clones — layer configs by reference, merge lazily.

### ntoml (4) / ncsv (4) / nvalid (9)
- ✨ `ntoml`: preserve comments/order on round-trip, datetime types. `ncsv`: quoting edge cases, streaming reader, type inference, custom delimiters. `nvalid`: async validators, cross-field rules, i18n messages, coerce+validate in one pass.
- ⚡ `ntoml` 27 push-sites, `ncsv` 18 — reserve buffers; `ncsv` SIMD comma/newline scan.

### ncron (4) / nshell (4) / nmarkdown (3) / nerrgen (3)
- ✨ `ncron`: seconds field + `@daily` macros + timezone. `nshell`: streaming stdout, env/cwd, pipeline chaining, interactive. `nmarkdown`: tables, code fences with lang, TOC, sanitized HTML. `nerrgen`: watch mode, per-lib code ranges validation.

### ncolor (24 builtins, 476 LOC) — `ncolor.rs`
- ✨ Add gradient/rgb interpolation, `supports_color` detection, theme presets, `hyperlink` OSC-8, progress-bar helpers.
- ⚡ Precompute the ANSI escape table (lazy `OnceLock`); avoid `format!` per styled span — write codes directly.

### nmem (14 builtins, 785 LOC) — `nmem.rs`
- ✨ Add vector-backed semantic recall (pair `nvec`), namespaces, size-based eviction, encryption at rest, export/import formats.
- ⚡ **45 clones (highest in the tree)** — store values behind `Rc`, return handles; clone only on explicit export.

### ncache (13 builtins, 561 LOC) — `ncache.rs`
- ✨ Add **LFU** eviction, **single-flight** (stampede protection), `get_many`/`set_many`, TTL jitter, weight-based size cap, hit-rate window stats.
- ⚡ Already O(log n) BTreeMap recency — offer an O(1) intrusive-list LRU option for hot caches.

### npipe (8 builtins, 560 LOC) — `npipe.rs`
- ✨ Add branch/merge, error short-circuit with context, async steps, typed step contracts (pair `ncontract`).
- ⚡ Precompile the step list; avoid boxing per element.

### nvis (8 builtins, 485 LOC) — `nvis.rs`
- ✨ Add SVG/PNG export, box/violin plots, subplots, axis formatting (pair `nfmt`), color themes (pair `ncolor`).
- 🐞 Manifest mojibake fixed.

### codec (7) / crypto (?) / archive (?) / args (6) — legacy crate-libs
- 🔧 **Consolidate.** `args`→`nargs`; fold `codec`/`crypto`/`archive` capabilities behind clearly-named native modules (nfmt/ncanon already cover some codec ground). Alias, migrate callers, delete dead paths.
- 📄 `crypto`, `archive` have **no doc** — draft stubs in `docs-proposed/`.

### nargs (4 builtins, 767 LOC) — `nargs.rs`
- ✨ Add subcommand `derive`-style spec, env-var fallback per option, mutually-exclusive groups, shell-completion generation.
- ⚡ 25 clones — parse into borrows of argv; build help text lazily (only on `--help`).

---

## F. Hardware

### ndevice (16) / ncpu (12) / ngpu (17) / nnpu (7) / nram (12)
- ✨ Add: unified `device.summary()` JSON; per-core frequency; GPU memory-bandwidth + multi-GPU enumerate; NPU vendor coverage (Apple ANE, Intel NPU, Qualcomm); RAM pressure events + swap stats.
- 🔧 `ndevice` should be the single front that composes `ncpu`/`ngpu`/`nnpu`/`nram` (avoid duplicate detection code paths).
- ⚡ Cache detection results (hardware doesn't change mid-run) behind a `OnceLock` + explicit `refresh()`.

---

## Sequencing suggestion

1. **Ship the defect fixes** (manifests) — zero-risk, unblocks tooling.
2. **`nsimd` `std::simd` upgrade**, then **`nvec` norms+SIMD** — biggest measured wins, and `nsimd` lifts ncl/nml/npar/nlazy.
3. **Allocation-discipline sweep** (json/ntoml/ncanon/dsa/nurl/ncsv) — mechanical, broad.
4. **Feature waves** by category, one lib at a time, each with a benchmark diff via `harness/`.
5. **Consolidation** (legacy crate-libs, resource-governor unification, shared deep-equal core) — lightweight + smaller surface.
