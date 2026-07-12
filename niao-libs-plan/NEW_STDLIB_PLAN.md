# Niao Stdlib Expansion — 10 New Developer Libraries

Goal: fill the everyday-developer gaps in the Niao standard library with **10 new native
modules**, all implemented std-only inside `crates/niao_runtime` — **zero new dependencies,
zero new crates, no Cargo.toml/Cargo.lock changes**. Every module follows the proven
`nenv.rs` pattern (flat builtins + namespace object) so it works in both the interpreter
and the VM automatically via `builtin_environment()`.

Design pillars: **fast** (no allocations where avoidable, packed arrays accepted directly),
**light RAM** (handle registries are thread-local, lazy eviction, no background threads),
**lightweight** (std-only, small code, no feature flags).

## The 10 libraries

| # | Lib | Import | Errors | What it gives developers |
|---|-----|--------|--------|--------------------------|
| 1 | `nstr` | `import "nstr"` | 2600–2603 | String toolkit: case conversions (upper/lower/title/snake/camel/kebab/pascal), trim/pad/wrap/dedent/indent, split/join/lines, contains/starts/ends/count, replace, repeat/reverse/truncate, slugify, levenshtein + similarity, is_* checks, chars/ord/chr |
| 2 | `nmath` | `import "nmath"` | 2610–2613 | Scalar math + stats: pi/e/tau/inf/nan, sqrt/cbrt/pow/exp/ln/log2/log10, full trig + atan2, floor/ceil/round/trunc, abs/sign/clamp/lerp/map_range, gcd/lcm/factorial/comb/perm, deg/rad, is_nan/is_finite, sum/mean/median/mode/variance/stdev/percentile/min/max over any array kind |
| 3 | `nrand` | `import "nrand"` | 2620–2623 | Random: xoshiro256** PRNG (SplitMix64 seeded), int/float/bool/range, bytes/hex/string/alphanum, choice/weighted/shuffle/sample, normal/exponential, isolated seeded generator handles for reproducibility |
| 4 | `nfmt` | `import "nfmt"` | 2630–2632 | Formatting: `fmt("{} {name}", ...)` templates, number precision + thousands separators, hex/oct/bin, pad/align/center, humanize bytes/duration/count, percent/currency |
| 5 | `nlog` | `import "nlog"` | 2640–2642 | Logging: trace..error levels, structured key-value fields, text or JSON lines, stderr/file sinks, timestamps, per-call fields + global context fields, zero cost below level |
| 6 | `nargs` | `import "nargs"` | 2650–2652 | CLI parsing: declarative spec (flags/options/positionals, types, defaults, aliases, required), auto `--help` usage text, `--key=value` and `-abc` bundling, typed results object |
| 7 | `ntest` | `import "ntest"` | 2660–2662 | Testing: `ntest.case(name, fn)` registration, `ntest.run()` runner with catch-per-test, assert_eq/ne/true/false/near/contains/error, summary object + exit code |
| 8 | `ncache` | `import "ncache"` | 2670–2672 | Caching: LRU cache (capacity), TTL cache (ms expiry, lazy eviction), get/set/has/remove/clear/len, hit/miss stats, handle-based like nenv stores |
| 9 | `nvalid` | `import "nvalid"` | 2680–2682 | Validation: schema objects (type/required/min/max/min_len/max_len/one_of/pattern), built-in fast checks email/url/uuid/ipv4/int/float/alnum, `check()` → {ok, errors}, `assert_valid()` |
| 10 | `ncolor` | `import "ncolor"` | 2690–2691 | Terminal styling: 16 named colors fg/bg, bold/dim/italic/underline, 256-color + truecolor RGB, style() composite, strip(), NO_COLOR / enabled toggle |

## Per-library integration checklist (all 8 steps required, per lib)

1. `crates/niao_runtime/src/<name>.rs` — module: helpers, native fns, `all_builtins()`,
   `namespace()`, `MODULE_NAME`, `MODULE_PATHS`, `builtins()`, `#[cfg(test)]` unit tests.
2. `crates/niao_errors/src/codes.rs` — error consts + `runtime_kind_name` range arm.
3. `crates/niao_runtime/src/lib.rs` — `mod <name>;`, `builtins.extend(<name>::builtins())`,
   `env.define(<name>::MODULE_NAME, ...)`, add to `native_module_paths()` (both cfg variants)
   and `native_module_export_name()`.
4. `crates/niao_pkg/src/catalog.rs` — `standard_libs()` entry (name, description,
   import paths, builtin_count).
5. `niao_libs/<name>/package.json` + `niao_libs/<name>/0.2.2/lib.json` + `catalog.json` libs list.
6. `docs/<NAME>.md` — API reference with examples.
7. `examples/<name>_demo.niao` — runnable demo.
8. Conventions: hard errors (bad arity/type) → `RuntimeError::at(span, code, ...)`;
   recoverable domain failures → `Ok(error_value(code, "<lib>_error", msg, span))` so
   `is_error()` / try-catch work. Flat builtin names are `<lib>_<fn>` (no collisions with
   dsa's generic names). Namespace keys are short (`nstr.upper`).

## Error code map (new block, 2600–2699)

```
nstr   2600 arity, 2601 error, 2602 type, 2603 bounds
nmath  2610 arity, 2611 error, 2612 type, 2613 domain
nrand  2620 arity, 2621 error, 2622 type, 2623 invalid handle
nfmt   2630 arity, 2631 error, 2632 type
nlog   2640 arity, 2641 error, 2642 type
nargs  2650 arity, 2651 parse error, 2652 spec error
ntest  2660 arity, 2661 error, 2662 assert failed
ncache 2670 arity, 2671 error, 2672 invalid handle
nvalid 2680 arity, 2681 error, 2682 schema error
ncolor 2690 arity, 2691 type
```

## Performance / RAM notes

- `nrand` xoshiro256** ≈ 0.8 ns/u64, 32-byte state; thread-local default generator, no locks.
- `ncache` LRU = HashMap + BTreeMap<u64 tick, key> recency index — O(log n) touch, O(log n)
  evict, no linked-list unsafe code; TTL eviction is lazy (on access) + `purge()`.
- `nstr`/`nfmt` operate on `char` iterators — Unicode-correct without extra allocation passes.
- `nmath` stats accept `IntArray`/`FloatArray` packed values directly (no boxing walk).
- `nlog` checks level with one atomic load before formatting anything.
- `ntest`/`ncache`/`nrand` handle registries are `thread_local!` `RefCell<HashMap<i64, ...>>`
  matching the nenv-store pattern; nlog global state is a `Mutex` (log from worker threads).

## Build order

nstr → nmath → nrand → nfmt → nlog → nargs → ntest → ncache → nvalid → ncolor
(then wiring, catalog, packages, docs, examples, review).

## Later candidates (not in this batch)

nsemver (version parse/compare), nurl (dedicated URL lib; net covers basics), ntmpl
(HTML-safe templating for ahiru), nmarkdown, ncron/nsched, nprompt (interactive TTY input),
nbench (micro-benchmark harness), nbig (arbitrary-precision decimal).
