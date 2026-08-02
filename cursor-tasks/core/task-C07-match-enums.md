# Task C07 — `match` and Enums

**Goal:** give Niao sum types and exhaustive dispatch.

**Read first:** `cursor-tasks/core/MASTER_PLAN.md`, `.cursor/rules/niao-core.mdc`.
**Prerequisite:** C05 Phase 3 (type checking) — exhaustiveness checking is a sema feature,
and without it `match` is just a `switch`.

---

## The problem

`niao_ast::Stmt` has no `Match`. `TopLevel` has no `Enum`. The lexer has neither keyword.

Every "one of N things" is currently an if-else chain over strings or ints, with no
compiler help when a case is missed and no way to attach data to a variant. Combined with
the absence of `Option`, this means "a value that might not be there" is expressed as
`nil` and checked by hand, everywhere, forever.

## Design decisions — write them in `docs/DECISIONS.md` first

1. **Enum shape.** Recommended: Rust-style with payloads, since you already have structs
   and the VM already has tagged instances:
   ```niao
   enum Shape {
       Circle(float),
       Rect(float, float),
       Empty,
   }
   ```
   C-style integer enums are cheaper but leave `Option`/`Result` unexpressible, which is
   most of the value here.
2. **`match` as statement, expression, or both.** Recommended: **both** — expression form
   is what makes it worth having (`let area = match s { ... }`). If you only have time for
   one, do the expression form.
3. **Exhaustiveness.** Recommended: **required**, with `_` as the explicit opt-out. A
   non-exhaustive `match` is a compile error naming the missing variants. This is the whole
   point of the feature — do not make it a warning.
4. **Pattern grammar.** Start small and land it: literals, variant patterns with binding
   (`Circle(r)`), `_`, and or-patterns (`A | B`). **Defer** guards (`if cond`), nested
   destructuring, range patterns, and struct patterns to a follow-up. Write the deferred
   list into the task summary so it is not lost.
5. **`Option` and `Result` in the prelude.** Recommended yes — define them as built-in
   enums once `enum` exists. This is what converts `match` from a nice feature into a
   change in how the language handles absence and failure. It may be worth its own task
   (`C07b`) since it touches every library's error convention.

## Implementation

- `niao_lexer`: `enum`, `match`, and `=>` (or `:` — pick one, `=>` is conventional).
- `niao_ast`: `TopLevel::Enum(EnumDef)`, `Stmt::Match`, `Expr::Match`, and a `Pattern` enum.
- `niao_parser`: enum declarations, match arms, pattern parsing.
- `niao_sema`:
  - register enum variants and their payload arities/types
  - type-check patterns against the scrutinee type
  - **exhaustiveness checking** — a usefulness algorithm over the variant set. Maranget's
    "Warnings for pattern matching" is the standard reference and it is short.
  - reachability: an arm shadowed by an earlier arm is a warning
  - binding patterns introduce correctly-typed variables into the arm's scope
- `niao_ir` / `niao_bytecode`: variant tag + payload representation; a `MatchTag` /
  jump-table lowering, not a linear chain of comparisons. Dense tags → jump table; sparse →
  binary search. Getting this right is what makes `match` faster than the if-else chain it
  replaces.
- `niao_vm` and `niao_interpreter`: identical semantics, including binding order and arm
  evaluation order.
- **GC:** enum payloads are heap values and are new roots. Add heap tests, as in C06.
- `niao_format`, `niao_lint`, `niao_docs`, `vscode-niao/syntaxes/`, `docs/grammar.ebnf`.

## Tests

- Every pattern form, matched and not matched.
- **Exhaustiveness: the negative tests matter most.** A match missing one variant must fail
  with a message naming *which* variant. Test with 2, 3, and 10 variants, and with `_`
  present and absent.
- Unreachable arm → warning with a span.
- Binding: payload values bound correctly, including multi-field variants and or-patterns
  binding the same names on both sides.
- Match as an expression: all arms must unify to one type; a mismatch is a typed error.
- Match on `int`, `string`, `bool`, and enums — literal patterns as well as variant ones.
- Nested match inside an arm.
- Enum equality and printing.
- Jump-table lowering: assert the emitted bytecode for a dense 8-variant match is a table,
  not 8 comparisons.
- Conformance cases for all of the above.

---

## Acceptance

- [ ] `enum` with payloads works in both engines.
- [ ] `match` works as both statement and expression, identically in both engines.
- [ ] Non-exhaustive `match` is a **compile error** naming the missing variants.
- [ ] Unreachable arms warn.
- [ ] Dense matches lower to a jump table; there is a test asserting it.
- [ ] GC handles enum payloads; heap tests pass.
- [ ] 100+ tests, majority negative cases asserting code + position.
- [ ] Conformance cases added; both engines agree.
- [ ] `examples/match_demo.niao` and a `Shape`-area example that would have been an
      if-else chain before.
- [ ] `docs/DECISIONS.md` records all five decisions plus the deferred pattern list.
- [ ] `docs/grammar.ebnf` updated.
- [ ] `cargo check --workspace`, `cargo test --workspace`, `niao test tests` green.
- [ ] `CHANGELOG.md` updated.
- [ ] `match` over 8 variants benchmarks **faster** than the equivalent if-else chain. If
      it does not, the lowering is wrong.
