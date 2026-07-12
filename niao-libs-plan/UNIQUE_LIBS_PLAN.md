# Niao Unique Libraries — Differentiating Stdlib Batch

Goal: ship libraries other languages rarely have as first-class stdlib.
All modules are **std-only**, zero new Rust crates, fast + lightweight.
Pattern: follow `ncache.rs` / `nvalid.rs` exactly.

Each subagent creates ONLY:
- `crates/niao_runtime/src/<lib>.rs`
- `docs/<LIB>.md`
- `examples/<lib>_demo.niao`
- `niao_libs/<lib>/package.json`
- `niao_libs/<lib>/0.2.2/lib.json`

**DO NOT** modify `lib.rs`, `codes.rs`, or `catalog.json` — parent wires those.

## Error code map (2940–3159)

```
nbudget   2940 arity, 2941 error, 2942 type, 2943 exceed
ncost     2950 arity, 2951 error, 2952 type
ncassette 2960 arity, 2961 error, 2962 type, 2963 invalid handle
nwhy      2970 arity, 2971 error, 2972 type, 2973 invalid handle
ncap      2980 arity, 2981 error, 2982 type, 2983 denied
nagent    2990 arity, 2991 error, 2992 type, 2993 invalid handle
nsketch   3000 arity, 3001 error, 3002 type, 3003 invalid handle
nexplain  3010 arity, 3011 error, 3012 type
npace     3020 arity, 3021 error, 3022 type
nbatch    3030 arity, 3031 error, 3032 type
nfallback 3040 arity, 3041 error, 3042 type
nmem      3050 arity, 3051 error, 3052 type, 3053 invalid handle
ndiff     3060 arity, 3061 error, 3062 type
ncanon    3070 arity, 3071 error, 3072 type
ncontract 3080 arity, 3081 error, 3082 type
nquota    3090 arity, 3091 error, 3092 type, 3093 invalid handle
nwatch    3100 arity, 3101 error, 3102 type, 3103 invalid handle
nfuzz     3110 arity, 3111 error, 3112 type, 3113 invalid handle
nshape    3120 arity, 3121 error, 3122 type
npipe     3130 arity, 3131 error, 3132 type, 3133 invalid handle
nreplay   3140 arity, 3141 error, 3142 type, 3143 invalid handle
nprofile  3150 arity, 3151 error, 3152 type
```

## Shared conventions

- Handles = `Value::Int(id)` + `thread_local!` registry (like ncache)
- Hard errors → `RuntimeError::at(span, code, …)`
- Recoverable → `error_value(code, "<lib>_error", msg, span)`
- `MODULE_NAME`, `MODULE_PATHS`, `builtins()`, `namespace()`
- Flat + short names via macro like nvalid
- `#[cfg(test)]` unit tests in module
- Version `0.2.2`, kind `native`
- Demo must `import "<lib>"` and call main APIs
- Keep APIs small (8–18 builtins), no async runtime, no new deps

## Library specs (APIs)

### 1. nbudget — unified resource + cost budgets
- `set({cpu_pct?, ram_mb?, gpu_pct?, usd?, tokens?})` / `get()` / `clear()`
- `check(extra?)` → `{ok, violations: [...]}`
- `ok()` bool · `remain()` leftover · `charge(kind, amount)` · `guard(fn)` skips if !ok
- Global thread_local budget state; charge is cooperative accounting

### 2. ncost — preflight LLM/cloud $ estimate
- `price(model, tokens_in, tokens_out?)` → usd float
- `estimate({model?, tokens_in?, tokens_out?, s3_gb?, lambda_ms?, requests?})` → `{usd, breakdown}`
- `table()` known prices · `set_price(model, in_per_mtok, out_per_mtok)`
- Built-in rough table: gpt-4o, gpt-4o-mini, claude-sonnet, llama-local=0

### 3. ncassette — record/replay HTTP/LLM exchanges
- `new(mode)` mode `"record"|"replay"|"passthrough"` → handle
- `key(method, url, body?)` · `put(h, key, response)` · `get(h, key)`
- `save(h, path)` / `load(path)` JSON file of `{key: response}`
- `wrap(h, key, fn)` — replay if hit else call fn and optionally record
- `close(h)` · `len(h)` · `keys(h)`

### 4. nwhy — value lineage / provenance
- `track(value, label)` → handle wrapping value + node id
- `derive(inputs_array, value, op_label)` · `value(h)` · `label(h)`
- `parents(h)` · `explain(h)` → string path · `graph(h)` → nodes/edges object
- `same(a,b)` · `close(h)`

### 5. ncap — capability sandbox (cooperative)
- `grant(["net","fs","env","process","gpu"])` / `revoke(...)` / `list()`
- `check(cap)` bool · `require(cap)` error if missing
- `with(caps_array, fn)` temporarily set · `deny_all()` / `allow_all()`
- Default: allow_all for back-compat; scripts opt into deny_all

### 6. nagent — lightweight multi-agent orchestration
- `new(name, role?, tools?)` → handle · `step(h, input)` appends message, returns last
- `messages(h)` · `remember(h, key, val)` · `recall(h, key)`
- `handoff(from, to, msg)` · `run(agents_array, kickoff, max_steps?)`
- `close(h)` · `name(h)` · `role(h)`

### 7. nsketch — probabilistic structures
- `bloom_new(n, fp?)` · `bloom_add` · `bloom_may_contain` · `bloom_clear`
- `hll_new()` · `hll_add` · `hll_count` (HyperLogLog-lite, 64 regs ok)
- `cms_new(w, d)` · `cms_add` · `cms_estimate` (Count-Min Sketch)
- `close(h)` · `kind(h)`
- Pure Rust, FNV/murmur-ish hash, no deps

### 8. nexplain — actionable error enrichment
- `of(err_or_msg)` → `{message, hint, fix, code?}`
- `register(pattern, hint, fix?)` · `hints()` list
- `format(err)` pretty string · Built-in hints for common Niao errors
- Pure string/object — no VM hooks

### 9. npace — adaptive loop pacing
- `set_level(0..3)` / `level()` · `sleep_ms()` returns delay for level (0/2/8/25)
- `tick()` sleeps current delay · `with_level(n, fn)`
- `from_temp(c, max)` maps temp→level · `from_load(pct)` maps load→level

### 10. nbatch — adaptive batch sizing
- `suggest(vram_mb?, ram_mb?, item_bytes?, max?)` → int
- `fit(total_items, batch)` → number of steps
- `clamp(n, min, max)` · `scale(n, factor)` · `halve_on(ok_bool, n)`

### 11. nfallback — graceful degradation chains
- `first(array_of_values)` first non-nil non-error
- `coalesce(...)` varargs · `try_chain(results_array)` 
- `circuit(name, fn_result, {fail_threshold?, reset_ms?})` trip open on failures
- `is_open(name)` · `reset(name)` · `or(a, b)` prefer a unless error/nil

### 12. nmem — script long-term memory (KV + TTL + tags)
- `new(capacity?)` · `set(h,k,v,ttl?)` · `get` · `has` · `remove` · `clear`
- `tag(h,k,tag)` · `by_tag(h,tag)` · `search(h, substr)` key substring
- `stats(h)` · `close(h)` · `export(h)` / `import(h, obj)`

### 13. ndiff — deep structural diff
- `diff(a, b)` → `{equal, changes:[{path, left, right}]}`
- `equal(a, b)` · `patch_summary(diff)` string
- Works on int/float/string/bool/nil/array/object (recursive)

### 14. ncanon — canonicalize + stable hash
- `canon(value)` → canonical JSON-ish string (sorted object keys)
- `hash(value)` → hex string (FNV-1a 64 or sip-free)
- `equal(a,b)` via hash · `fingerprint(value)` short prefix

### 15. ncontract — design-by-contract
- `require(cond, msg?)` throw if false · `ensure(cond, msg?)`
- `check(cond, msg?)` → error value if false else true
- `invariant(obj, rules_obj)` like nvalid subset · `assert_type(v, type_str)`

### 16. nquota — uniform rate limits
- `new(rate_per_sec, burst?)` token bucket → handle
- `take(h, n?)` bool · `wait_ms(h)` suggested sleep · `ok(h)`
- `reset(h)` · `stats(h)` · `close(h)`

### 17. nwatch — reactive poll watchers
- `file(path)` handle · `changed(h)` bool (mtime) · `poll(h)` 
- `value(init)` · `set(h,v)` · `take_changed(h)` 
- `close(h)` · `path(h)`

### 18. nfuzz — property / fuzz helpers
- `seed(n)` · `int(min,max)` · `float(min,max)` · `bool()` · `string(len?)`
- `pick(array)` · `shuffle(array)` copy · `bytes(n)`
- `cases(n, gen_desc)` — for int ranges produce n samples
- Deterministic after seed; thread_local RNG (xorshift)

### 19. nshape — shape / schema diagrams
- `of(value)` → string like `{name: string, tags: array[3]}`
- `check(value, shape_str_or_obj)` → `{ok, errors}`
- ` Rank(arr)` · `dims(arr)` for packed/arrays · `match(a,b)` same shape?

### 20. npipe — typed step pipelines
- `new()` · `add(h, name, fn_or_nil)` · `run(h, input)` sequential
- `steps(h)` · `clear(h)` · `close(h)`
- Note: native can't call Niao fns easily — store NativeFunction OR just store transforms as labeled stages where `run` applies identity and returns `{step, input}` for host orchestration; better: accept only values and use `map`-style if Function available.
- Practical API: `run(stages_array, input)` where each stage is `{name, fn}` NativeFunction/Function if callable via runtime — **if Function calling from native is hard, implement `npipe_plan` + `npipe_describe` only and `apply(name, value)` registry of built-in ops: `id`, `len`, `type`, `str`, `json_keys`.**
- Use built-in op registry: `register(name, op)` where op in `id|len|keys|type|not_nil` plus `run(ops_array, input)`

### 21. nreplay — deterministic record of time/RNG/events
- `start()` / `stop()` → handle session
- `record(h, kind, data)` · `events(h)` · `len(h)`
- `save(h, path)` / `load(path)` · `play(h, i)` get event i
- `close(h)` · `clear(h)`

### 22. nprofile — micro timing / spans
- `start(label)` → handle · `end(h)` → `{label, ms}`
- `time(label)` wall ms since start label map · `bench(iters, fn_result_ignored)` — if can't call fn, `bench_ms(samples_array)` stats
- `stats(ms_array)` → `{n, mean, min, max, p50, p95}`
- `now_ms()` · `span(label)` push/pop nest

## Integration (parent only, after all land)

- Add codes to `codes.rs` + `runtime_kind_name`
- `mod` + `builtins.extend` + `env.define` + path lists + `export_native_module` in `lib.rs`
- Append names to `niao_libs/catalog.json`
