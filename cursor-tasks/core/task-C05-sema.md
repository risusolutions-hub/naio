# Task C05 — `niao_sema`: the Type Checker

**Goal:** make Niao's type annotations mean something. This is the unlock — better errors,
real optimisation, and the LSP all depend on it.

**Read first:** `cursor-tasks/core/MASTER_PLAN.md`, `.cursor/rules/niao-core.mdc`.
**Prerequisite:** C04 green. **Do not start this without the C03 conformance harness.**

This is a multi-week task. Land it in the phases below, each independently green and
shippable. Do not attempt it in one commit.

---

## The problem

`TypeName::` appears in `niao_parser`, `niao_format`, `niao_docs`, `niao_lint`, and
`niao_reflect`. It appears in `niao_ir`, `niao_bytecode`, `niao_vm`, and
`niao_interpreter` **exactly zero times.**

Type annotations are parsed and discarded. This compiles and runs:

```niao
fn add(a: int, b: int) -> int { return a + b }
fn main() { print(add("hello", [1, 2])) }
```

`README.md` says "Niao uses gradual static typing." Today that sentence is not backed by an
implementation. This task makes it true.

## Why this is also the performance task

Every optimisation that would make Niao genuinely fast is blocked on knowing types:

- Values are tagged; every arithmetic op dispatches on the tag at runtime.
- Locals are boxed because their types are unknown.
- Field access is a name lookup instead of a slot index.
- Global lookups hash a string in the dispatch loop.
- Inline caches are guesses rather than guarantees.

Phase 5 below is where that gets cashed in. Do not skip it — it is the reason this task
outranks adding more libraries.

---

## Phase 1 — The crate and the type lattice

Create `crates/niao_sema`, sitting between `niao_parser` and `niao_ir`. Add it to the
workspace members and `[workspace.dependencies]` in the root `Cargo.toml`.

`TypeName` in `niao_ast` is too weak to check against — it is a flat enum with `Named(String)`
and an untyped `Array`. Define a real `Ty` in `niao_sema`:

- Primitives: `Int`, `Float`, `Bool`, `String`, `Void`, `Nil`
- `Array(Box<Ty>)` — element type, so `array` alone becomes `Array(Unknown)`
- `Map(Box<Ty>, Box<Ty>)` — object literals have no type at all today
- `Fn(Vec<Ty>, Box<Ty>)` — needed by C06, define it now
- `Struct(SymbolId)`, `Class(SymbolId)`, `Trait(SymbolId)`
- `Error` — the thrown-value type
- **`Unknown`** — the gradual-typing escape hatch. Unannotated things get this, and
  `Unknown` is compatible with everything in both directions. This is what makes adoption
  incremental: existing untyped programs keep working unchanged.
- **`Never`** — the type of `throw` and of a block that always returns. Needed for
  correct exhaustiveness and reachability.

Assignability (`Unknown` compatible both ways; `Nil` assignable to any nullable position;
`int` → `float` **only** via explicit widening, never implicitly — implicit numeric coercion
is a bug factory). Write the rules in `docs/DECISIONS.md` **before** implementing them.

Keep `niao_ast::TypeName` as the pure syntactic form; `Ty` is the semantic form. Do not
merge them.

## Phase 2 — Symbol resolution (ship this alone; it is valuable without types)

Walk the `Program` and build scopes: globals, functions, params, locals, classes, structs,
traits, and their members. Resolve every `Expr::Ident` to a `SymbolId`.

This alone catches, at compile time, things that are currently runtime errors or silent:

- undefined variable → `E0300_UNDEFINED_NAME`, with a "did you mean …?" suggestion from
  edit distance over in-scope names
- undefined function, undefined struct/class in an initialiser
- duplicate parameter, duplicate field, duplicate top-level definition
- use-before-definition inside a function body
- unknown field in a struct or class initialiser, and missing required fields
- `self` outside a method, `super` outside a class with `extends`
- `break`/`continue` outside a loop (currently `E1005`, caught at runtime — move it here)
- unreachable code after `return`/`throw`
- **unused variables and unused imports** → `W....` warnings, wired into `niao_lint`

Allocate `E0300`–`E0399` for resolution and `E0400`–`E0499` for typing in
`crates/niao_errors/src/codes.rs`, with a header comment extending the band table.

**Ship Phase 2 on its own.** It is a large, immediate quality win and it de-risks Phase 3.

## Phase 3 — Type checking

With names resolved, check:

- assignment compatibility on `let` with an annotation
- call arity (already partly at runtime) **and** argument types
- return type against every `return` in the body, including implicit fall-off-the-end
- binary/unary operand types, including the C04 bitwise int-only rules
- index types (`array[int]`, `map[K]`), member existence on structs and classes
- **trait conformance** — a class declaring `implements Foo` must actually have every
  method in `Foo` with a matching signature. Currently `register_metadata` stores traits
  and nothing verifies them.
- `extends` — no cycles, override signatures compatible with the parent

Local inference only: infer `let` from its initialiser; do **not** attempt whole-program
Hindley–Milner. Unannotated parameters are `Unknown`. Explicit is better than clever here.

Error quality is the deliverable, not just the detection. Every error carries: the code,
the narrowest span, expected vs actual type, and where the expectation came from ("`add`
declares `b: int` at line 3"). Look at how `rustc` and Elm phrase these.

## Phase 4 — Wire it in

- `niao run`, `niao build`, `niao test`, `niao serve`, `niao lint` all run sema after
  parsing and abort on errors.
- **A `--no-check` escape hatch**, plus a per-file `#!niao-check: off` directive. When you
  turn this on, existing programs in `examples/`, `tests/`, and `niao_libs/` will fail.
  Fix them. The ones you cannot fix quickly are findings — record them.
- The `Unknown`-is-compatible-with-everything rule should keep most untyped code passing.
  If it does not, the assignability rules are too strict — revisit Phase 1 rather than
  weakening the checker with special cases.

## Phase 5 — Cash in the performance

Emit a `TypedProgram` from sema and consume it in `niao_ir`:

- **Slot-resolved locals and globals.** Replace name-keyed lookup with indices. This is the
  single biggest win and it needs only Phase 2, not Phase 3.
- **Specialised arithmetic opcodes** — `AddInt`/`AddFloat` where both operands are statically
  known, skipping the tag check.
- **Unboxed `int`/`float`/`bool` locals** where the type is known and never `Unknown`.
- **Direct field slots** for struct and class field access instead of name lookup.
- **Static call resolution** — direct dispatch when the callee is statically known.

Benchmark each of these separately against `benchmarks/baseline.json` and put the numbers
in the task summary. This is where the "fastest language" claim gets evidence.

---

## Acceptance

- [ ] `crates/niao_sema` exists, zero new third-party dependencies.
- [ ] The `add("hello", [1,2])` example above fails at compile time with a code, a span,
      and a message naming both the expected and the actual type.
- [ ] Trait conformance is verified; a class with a missing trait method fails to compile.
- [ ] Phases 2 and 3 each have 150+ tests, mostly negative cases asserting code + position.
- [ ] Every existing `.niao` file in `examples/`, `tests/`, and `niao_libs/` either passes
      the checker or is explicitly listed as a known gap in the summary.
- [ ] `--no-check` works for escape.
- [ ] Conformance suite still green — sema must not change runtime behaviour of programs
      that already type-check.
- [ ] Phase 5 shows a measured speedup on at least three benchmarks, with numbers.
- [ ] `README.md`'s "gradual static typing" claim is now accurate.
- [ ] `docs/DECISIONS.md` records the assignability rules and the inference boundary.
- [ ] `cargo check --workspace`, `cargo test --workspace`, `niao test tests` green.
- [ ] `CHANGELOG.md` updated.

**Ship Phase 2 before starting Phase 3.** A resolver that lands is worth more than a type
checker that is 80% done in a branch.
