# nschema standard library

Infer JSON Schema from example values, validate/coerce/parse structured data, and emit LLM prompt snippets and tool specs. Pairs with `nagent` for offline agent tool wiring.

## Import

```niao
import "nschema"
```

Paths `import "std/nschema"` and `import "nschema"` are equivalent.

## Quick start

```niao
import "nschema"
import "nagent"

let example = {name: "Niao", age: 3, tags: ["cute", "fast"]}
let schema = nschema.from_example(example)

let raw = '{"name":"Bob","age":"42"}'
let user = nschema.parse(raw, schema)
print(user.age)   // 42 (coerced int)

let check = nschema.validate(user, schema)
if !check.ok { for e in check.errors { print(e) } }

let tool = nschema.tool("search", "Search documents", {
    type: "object",
    properties: {query: {type: "string"}},
    required: ["query"]
})
let agent = nagent.new("researcher")
nagent.remember(agent, "search_tool", tool)
print(nschema.prompt(schema, "Extract user profile JSON."))
```

## Schema format

JSON-Schema-lite objects:

| Field | Description |
|-------|-------------|
| `type` | `null`, `boolean`, `integer`, `number`, `string`, `array`, `object`. |
| `properties` | Object field rules (object type). |
| `required` | Array of required property names. |
| `items` | Element schema (array type). |
| `min` / `max` | Numeric bounds. |
| `min_len` / `max_len` | String length bounds. |
| `pattern` | Regex (compiled + cached). |
| `one_of` | Value must deep-equal one option. |

`from_example` infers types from a sample Niao value (strings that look numeric become `integer` / `number`).

## Functions

| Method | Description |
|--------|-------------|
| `nschema.from_example(value)` | Infer schema from an example value. |
| `nschema.validate(value, schema)` | `{ok: bool, errors: [path: message, …]}`. |
| `nschema.coerce(value, schema)` | Coerce strings → numbers/bools, recurse into objects/arrays; catchable error on failure. |
| `nschema.parse(json_str, schema)` | `json.parse` + `coerce`. |
| `nschema.prompt(schema, title?)` | LLM instruction block with embedded JSON schema (no markdown fences). |
| `nschema.tool(name, description, schema)` | `{name, description, parameters}` — OpenAI-style tool spec for agent memory / orchestration. |

## Errors

| Code | Meaning |
|------|---------|
| 3290 | Wrong argument count. |
| 3291 | Invalid pattern, empty tool name, etc. (catchable). |
| 3292 | Wrong argument type. |
| 3293 | Validation / coerce / parse failure (catchable). |
