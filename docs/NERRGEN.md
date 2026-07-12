# nerrgen standard library

Parse E-code specification files and generate Rust constants, Niao tables, and Markdown documentation.

## Import

```niao
import "nerrgen"
```

Paths `import "std/nerrgen"` and `import "nerrgen"` are equivalent.

## Quick start

```niao
import "nerrgen"

let spec = """
# nsemver errors
E2900 nsemver_arity | Wrong argument count | nsemver_error
E2901 nsemver_error | Semantic error | nsemver_error
E2902 nsemver_parse | Parse error | nsemver_error
"""

let entries = nerrgen.parse(spec)
let rust = nerrgen.gen(entries, "rust")
let all = nerrgen.all(entries, "NSEMVER errors")
print(all.rust)
print(all.niao)
print(all.markdown)
```

## Spec format

One entry per line ( `#` comments and blank lines are ignored):

```
E2900 nsemver_arity | Wrong argument count | nsemver_error
2901 nsemver_error Semantic error nsemver_error
```

Fields: **code** (`E` prefix optional), **name** (snake_case identifier), **message**, optional **kind** (defaults to `<prefix>_error` from name).

## Functions

| Method | Description |
|--------|-------------|
| `nerrgen.parse(spec)` | Parse spec text → array of `{code, name, message, kind, line}`. Catchable `error` on invalid spec. |
| `nerrgen.gen(entries, format)` | Generate artifact string. `format` is `rust`, `niao`, or `markdown` / `md`. |
| `nerrgen.all(entries, title?)` | Returns `{rust, niao, markdown}` in one object. |

`entries` may be the raw spec string or the array from `parse`.

## Generated artifacts

- **rust** — `pub const E####_NAME: u32 = ####;` plus a `generated_kind` match helper.
- **niao** — array literal of `{code, name, message, kind}` objects.
- **markdown** — table of codes plus an errors summary section.

## Errors

| Code | Meaning |
|------|---------|
| 3240 | Wrong argument count. |
| 3241 | Parse / generation error (catchable). |
| 3242 | Type error. |
