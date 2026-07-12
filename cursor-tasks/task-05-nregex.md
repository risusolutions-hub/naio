# Task 05 — nregex: own regex engine (replace `regex`)
Read MASTER_PLAN.md first. This is a big one — take it in one crate, careful scope.

## Build
Create `crates/niao_regex` (zero deps):
- Syntax: literals, classes [a-z\d], negation, ., anchors ^$, groups (capturing + (?:...)), alternation, quantifiers * + ? {m,n} (greedy+lazy), escapes \d \w \s \b, unicode-aware on char level (no full UTS#18).
- Engine: parse → NFA → Pike VM (thompson simulation, O(n*m), no catastrophic backtracking) with capture slots. Add a literal/prefix fast-path (memchr-style scan written by hand).
- API mirror of regex crate subset: is_match, find, find_iter, captures, replace_all, split. Compiled-pattern cache (LRU) in runtime.

## Wire up
- Replace regex in niao_runtime/re.rs (keep the Niao-facing `re` lib API identical) and workspace-root usage.

## Acceptance
- Port the existing re.rs tests + add 60 cases incl. pathological patterns ((a+)+b on long input must stay linear).
- Bench: within 2x of regex crate on our benchmarks/ workloads (regex crate is world-class; 2x is fine for v1), and no exponential blowups.

## Ground rules (apply to EVERY task)
- ZERO new third-party crates. Only std + existing niao_* workspace crates.
- Lightweight & fast: no allocations in hot loops, prefer &str/&[u8], pre-size Vecs, add #[inline] on small hot fns.
- Public API surface goes to Niao programs via niao_runtime builtins + a niao_libs/<name>/ module (Niao-language wrapper), mirroring how niao_libs/json works today.
- Add unit tests in the crate + at least one .niao example under examples/.
- Add/extend a benchmark under benchmarks/ comparing before vs after.
- After changes: `cargo check --workspace` then `cargo test --workspace` must pass.
- Remove the replaced third-party dependency from every Cargo.toml that used it, and confirm it disappears from `cargo tree`.
- Update CHANGELOG.md with one line.
