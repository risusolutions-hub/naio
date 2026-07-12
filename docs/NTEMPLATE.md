# ntemplate standard library

Versioned prompt templates with `{{var}}` injection and token-count estimation for context budgeting. Distinct from `nprompt` (interactive CLI prompts).

## Import

```niao
import "ntemplate"
```

Paths `import "std/ntemplate"` and `import "ntemplate"` are equivalent. Flat builtins (`ntemplate_set`, `ntemplate_render`, …) are also available globally after import.

## Quick start

```niao
import "ntemplate"

ntemplate.set("greet", "1.0.0", "Hello {{name}}, welcome to {{place}}.")
ntemplate.set("greet", "2.0.0", "Hey {{name}} — glad you're at {{place}}!")

let prompt = ntemplate.render("greet", { name: "Ada", place: "Neko" })
print(prompt)                              // uses latest version (2.0.0)
print(ntemplate.estimate(prompt))          // heuristic token count
```

Run: `niao run examples/ntemplate_demo.niao`

## Functions

| Method | Description |
|--------|-------------|
| `ntemplate.set(name, version, body)` | Register or update a versioned template. |
| `ntemplate.get(name, version?)` | Return template body. Without `version`, returns the highest semver-like version. |
| `ntemplate.versions(name)` | Sorted list of registered versions for `name`. |
| `ntemplate.vars(template)` | Extract unique `{{var}}` names from a template string. |
| `ntemplate.render_str(template, vars)` | One-shot render of an inline template object. |
| `ntemplate.render(name, vars, version?)` | Render a registered template with variable injection. |
| `ntemplate.estimate(text)` | Heuristic token estimate (~4 chars/token, min 1 for non-empty). |
| `ntemplate.remove(name, version?)` | Remove one version or all versions of `name`. Returns `true` if removed. |

### Variable syntax

Use `{{variable_name}}` (Mustache-style, whitespace inside braces is trimmed). Missing variables render as empty strings.

## Errors

| Code | Meaning |
|------|---------|
| 3300 | Wrong argument count. |
| 3301 | Template not found, unclosed `{{`, or other semantic error (catchable). |
| 3302 | Wrong argument type. |
