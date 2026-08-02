# nopenapi standard library

OpenAPI 3 spec generation from ahiru routes + typed Niao client stub generation. Native Rust implementation (~FastAPI `app.openapi()` + openapi-generator subset).

## Import

```niao
import "nopenapi"
```

Paths `import "std/nopenapi"` and `import "nopenapi"` are equivalent. Flat builtins (`nopenapi_from_ahiru`, `nopenapi_client_stub`, …) are also available globally after import.

## Quick start

```niao
import "nopenapi"

// From ahiru-style routes (`:id` → `{id}`, permissions → bearer security)
let routes = [
    {method: "GET", path: "/health", summary: "Health"},
    {method: "GET", path: "/users/:id", permission: "users.read"},
    {method: "POST", path: "/users", body: {name: "Ada", age: 30}},
]
let doc = nopenapi.from_ahiru(routes, {title: "Demo API", version: "1.0.0"})
print(nopenapi.is_valid(doc))          // true
print(nopenapi.paths(doc))             // ["/health", "/users", "/users/{id}"]

let stub = nopenapi.client_stub(doc, {base_url: "http://127.0.0.1:8080"})
// stub is Niao source that wraps http.get / http.post per operation

nopenapi.close(doc)
```

## Document lifecycle

| Method | Description |
|--------|-------------|
| `nopenapi.create(info, opts?)` | Empty OpenAPI 3 doc. `info`: `{title, version, …}`. `opts.openapi` defaults to `"3.1.0"`. |
| `nopenapi.parse(json_or_object)` | Parse a JSON string or object into a handle. |
| `nopenapi.load(path)` / `nopenapi.save(doc, path, pretty?)` | File IO. |
| `nopenapi.clone(doc)` / `nopenapi.close(doc)` | Copy / free handle. |
| `nopenapi.to_json(doc, pretty?)` | Serialize. |
| `nopenapi.to_object(doc)` | Niao object view of the full document. |
| `nopenapi.version(doc)` | OpenAPI version string. |

## Routes (ahiru / FastAPI-style)

| Method | Description |
|--------|-------------|
| `nopenapi.from_ahiru(routes, info?, opts?)` | Build from `{method, path, permission?, websocket?}` (+ optional `opts.enrich`). |
| `nopenapi.from_routes(routes, info?, opts?)` | Same with full operation descriptors. |
| `nopenapi.add_route(doc, route)` / `add_routes(doc, routes)` | Mutate. |
| `nopenapi.add_path(doc, path, method, operation)` | Low-level path item insert. |
| `nopenapi.normalize_path(path)` | `/users/:id` → `/users/{id}`. |
| `nopenapi.path_params(path)` | `["id", …]`. |
| `nopenapi.operation_id(method, path)` | FastAPI-style id (`get_users_by_id`). |

Route keys: `method`, `path`, plus optional `summary`, `description`, `tags`, `operationId`, `parameters`, `requestBody`, `request`/`body` (example → inferred schema), `responses`/`response`/`response_schema`, `security`, `deprecated`, `permission`, `websocket`.

## Components & helpers

| Method | Description |
|--------|-------------|
| `nopenapi.add_schema` / `add_security_scheme` / `add_component` / `add_server` / `add_tag` / `set_info` | Document builders. |
| `nopenapi.merge(a, b)` | Deep-merge paths/components (overlay wins). |
| `nopenapi.param` / `request_body` / `response` / `operation` | Operation fragment builders. |
| `nopenapi.schema_ref` / `schema_object` / `schema_array` / `schema_string` / `schema_integer` / `schema_number` / `schema_boolean` | Schema builders. |
| `nopenapi.infer_schema(value)` | Infer JSON Schema from an example value. |

## Introspection & validate

| Method | Description |
|--------|-------------|
| `nopenapi.paths` / `operations` / `get_operation` / `schemas` | Inspect. |
| `nopenapi.validate(doc)` | `{ok, errors: [{path, message}, …]}`. |
| `nopenapi.is_valid(doc)` | Boolean. |
| `nopenapi.parallel_validate(docs, opts?)` | Batch validate (`opts.threads`). |

## Client stubs (~openapi-gen)

| Method | Description |
|--------|-------------|
| `nopenapi.client_stub(doc, opts?)` | Emit Niao source wrapping `http` calls. |
| `nopenapi.client_niao(doc, opts?)` | Alias of `client_stub`. |
| `nopenapi.parallel_client_stubs(docs, opts?)` | Batch generate. |

`opts`: `{module, base_url, client_var, include_types, threads}`.

## Errors

| Code | Meaning |
|------|---------|
| 4120 | Wrong argument count. |
| 4121 | Catchable library error (`nopenapi_error`). |
| 4122 | Type mismatch (hard). |
| 4123 | Parse / JSON error (catchable). |
| 4124 | Invalid or closed document handle. |

## Deferred (v0.1)

YAML OpenAPI input/output, Swagger UI hosting, non-Niao code generators, live HTTP client from stubs, full OAS 3.1 JSON Schema dialect conformance suite.
