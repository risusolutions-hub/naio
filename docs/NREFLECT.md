# nreflect — runtime introspection

Runtime introspection: function arity and parameters, doc strings, loaded-module listing, source locations, and call-stack frames. A scoped port of Python `inspect` for Niao values and source text.

User functions defined in loaded `.niao` modules are registered automatically by the interpreter. Native builtins appear in `native_modules()`; arity for native callables is `nil` (same as `nfunc.arity`).

## Import

```niao
import "nreflect"
```

Paths `import "std/nreflect"` and `import "nreflect"` are equivalent. Flat builtins (`nreflect_arity`, `nreflect_doc`, …) are also available globally after import.

## Quick start

```niao
import "nreflect"

// Doc comment above a function is discoverable from source:
let src = "// Adds two ints.\nfn add(a: int, b: int) -> int { return a + b }"
print(nreflect.doc_from_source(src, "add"))   // "Adds two ints."

let add = fn(x, y) { return x + y }
print(nreflect.arity(add))                    // 2
print(nreflect.format_signature(add))         // "add(x, y)"
print(nreflect.is_callable(add))              // true

let info = nreflect.parse_module(src)
print(info.functions[0].name)                 // "add"

print(len(nreflect.native_modules()) > 0)    // true
```

## Type predicates

| Method | Description |
|--------|-------------|
| `nreflect.is_function(val)` | User `fn` value. |
| `nreflect.is_native(val)` | Native builtin callable. |
| `nreflect.is_callable(val)` | User or native function. |
| `nreflect.is_instance(val)` | OOP class instance. |
| `nreflect.kind(val)` | Structural kind string (`"int"`, `"function"`, …). |

## Function metadata

| Method | Description |
|--------|-------------|
| `nreflect.arity(fn)` | Parameter count; `nil` for native. |
| `nreflect.name(fn)` | Declared name or `"<native>"`. |
| `nreflect.params(fn)` | `[{name, type?, line, col}, …]`. |
| `nreflect.signature(fn)` | `{name, params, arity, return_type?, line, col, formatted}`. |
| `nreflect.return_type(fn)` | Return type name or `nil`. |
| `nreflect.format_signature(fn)` | Human-readable signature string. |

## Doc strings & source

| Method | Description |
|--------|-------------|
| `nreflect.doc(fn)` | Doc comment above a loaded user function, or `nil`. |
| `nreflect.doc_from_source(source, name)` | Extract `//` / `///` doc above a declaration. |
| `nreflect.source(fn)` | Source slice for a registered user function, or `nil`. |
| `nreflect.source_lines(fn)` | `{start, lines?}`. |
| `nreflect.source_file(fn)` | Module path where the function was defined. |
| `nreflect.location(fn)` | `{file?, line, col}`. |

Doctest lines (`// >>>`, `// =>`) are excluded from doc extraction.

## Modules

| Method | Description |
|--------|-------------|
| `nreflect.modules()` | Loaded file modules `[{path, exports, kind}]`. |
| `nreflect.native_modules()` | Native std import paths (short names). |
| `nreflect.module_info(path)` | Parsed summary or `nil`. |
| `nreflect.module_exports(path)` | Export names or `nil`. |
| `nreflect.parse_module(source)` | Parse without executing. |
| `nreflect.register_module(path, source)` | Manual module registration. |
| `nreflect.find_function(source, name)` | Lookup signature in source (catchable error if missing). |
| `nreflect.scan(items)` | Parallel parse of `[{path, source}, …]`. |

## Objects & stack

| Method | Description |
|--------|-------------|
| `nreflect.members(obj, kind?)` | Sorted keys on objects/instances; optional kind filter. |
| `nreflect.getmodule(fn)` | Defining module path or `nil`. |
| `nreflect.stack()` | Current call-stack frames `[{name, file?, line, col}]`. |
| `nreflect.current_frame()` | Innermost frame or `nil`. |
| `nreflect.clear()` | Reset module/function registry (mainly for tests). |

## Errors

| Code | Meaning |
|------|---------|
| 3517 | Wrong argument count. |
| 3518 | Not found / introspection failed (catchable `nreflect_error`). |
| 3519 | Wrong argument type (hard error). |
| 3520 | Reserved for not-found variants. |

## Deferred / limitations

- No generator/coroutine introspection (use `nasync` for tasks).
- No closure cell inspection (`inspect.getclosurevars` equivalent).
- Class attribute classification is limited to `members()` on instances.
- Native function docs are not embedded in the binary (use `docs/*.md` or source scan).
- VM-only execution paths may not populate the module registry unless modules are registered explicitly.

## See also

- `ndoc` — doctest extraction and execution (`// >>>` / `// =>`).
- `nfunc` — `arity()` helper for combinators.
- `nshape` — structural type/shape strings for values.
- `ndebug` — value diff and checkpoints.
