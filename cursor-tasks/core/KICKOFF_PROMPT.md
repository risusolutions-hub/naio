# Paste this into Cursor

Copy everything between the lines. Run one task per session — do not let the agent attempt
two at once. Review `git diff` after each.

---

You are working on **Niao**, a programming language implemented in Rust at `C:\Risu\Niao`.
Our goal is for Niao to be the fastest and best language of its class. Right now it isn't,
and we know exactly why.

## Read these first, in this order

1. `cursor-tasks/core/MASTER_PLAN.md` — the diagnosis and the plan
2. `.cursor/rules/niao-core.mdc` — the rules you must follow (auto-loads on core files)
3. `cursor-tasks/core/task-C01-foundation-ci.md` — your task

## The situation

We audited the repo. The finding:

> The ecosystem is 130 crates wide. The language is 1,136 lines of parser deep.

The whole compiler pipeline — lexer, parser, AST, IR, bytecode, VM, interpreter — is
~11,900 lines with **49 tests**. The 130 library crates around it have **2,056**. Every
library rests on a compiler that is barely tested, and the audit found real bugs sitting in
that gap:

- **There is no type checker.** `TypeName::` appears in the parser, formatter, and docs
  generator, and **zero times** in the IR, bytecode, VM, or interpreter. Annotations are
  parsed and thrown away. `add("hello", [1,2])` against `fn add(a: int, b: int)` compiles
  and runs.
- **`niao_ir` silently discards `struct`, `server`, and route blocks** via a `_ => {}`
  catch-all. `niao run` on a file with a `server {}` block exits 0 with no output.
- **There are no bitwise operators.** A lone `&` or `|` is a lex error. Hashing and bit
  manipulation cannot be written in Niao at all.
- **`//` is heuristically comment-or-floor-division.** `let x = 1 // (see note)` silently
  parses as division.
- **No closures.** `map`, `filter`, callbacks, and comparators are all inexpressible.
- **No CI, no LICENSE file** (README and Cargo.toml both claim MIT).

## The plan

Eight tasks, in `cursor-tasks/core/`, run in order:

| # | Task | Why |
|---|---|---|
| C01 | Foundation & CI | The safety net. Nothing else is safe without it. |
| C02 | Lexer correctness | Six real bugs; 7 tests → 100+ |
| C03 | Kill silent drops + conformance harness | Makes engine divergence visible |
| C04 | Operators | First feature landed through the new harness |
| C05 | `niao_sema` — the type checker | The unlock |
| C06 | Closures | Largest expressiveness gap |
| C07 | `match` + enums | Exhaustiveness and real error handling |
| C08 | Language spec + LSP | Adoption |

**On speed:** correctness first is not a detour from "fastest" — it is the road to it.
The VM cannot do the optimisations that make a language fast (unboxed locals, specialised
arithmetic, slot-resolved lookups, monomorphisation) because it does not know the types of
its values. C05 Phase 5 is where that gets cashed in, with benchmark numbers.

## Rules that apply to everything you do

Full list in `.cursor/rules/niao-core.mdc`. The ones people get wrong:

- **ZERO new third-party crates.** `std` + existing `niao_*` only. Hand-write it. This
  includes the LSP — no `tower-lsp`.
- **Two engines, one language.** `niao_vm` and `niao_interpreter` must behave identically.
  Semantics land in both in the **same commit**, with a conformance test proving it.
- **No `_ => {}`** in any AST/IR/opcode match. Unhandled constructs return a typed error
  from `crates/niao_errors/src/codes.rs`. Prefer exhaustive matches so the compiler catches
  the next person.
- **No panics on user input.** Malformed source produces a `NiaoError` with a code and a
  span pointing at the narrowest range that is actually wrong.
- **Tests are the deliverable.** A feature without tests is not done. Assert error *codes*
  and *line/col*, not just that something failed.
- **No perf regressions** against `benchmarks/baseline.json` without written justification
  and numbers.

## Definition of done for any task

```
cargo check --workspace
cargo test --workspace
cargo run --release --bin niao -- test tests
```

All three green, plus one line in `CHANGELOG.md`.

## Start here

Execute **`cursor-tasks/core/task-C01-foundation-ci.md`**.

Work through it section by section. When you finish, give me:

1. A summary of what changed, file by file.
2. Anything the task told you to record as a finding rather than fix.
3. Anything in the task that turned out to be wrong about the codebase — the audit was
   thorough but it was not exhaustive, and if reality disagrees with the task file, reality
   wins. Tell me and I will update the plan.

Do not start C02 in this session.

---

## Follow-up prompt for each subsequent task

> Execute `cursor-tasks/core/task-C0N-<name>.md`. Follow `.cursor/rules/niao-core.mdc`.
> All acceptance criteria must be met and `cargo test --workspace` plus `niao test tests`
> must be green before you call it done. Report what changed, what you deferred, and
> anything in the task file that was wrong about the codebase.
