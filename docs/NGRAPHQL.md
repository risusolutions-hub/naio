# ngraphql — GraphQL client + schema/server helpers

Parse and print GraphQL documents, build HTTP request bodies (~`gql`),
validate queries against SDL schemas, and execute map-based resolvers
(~graphene / strawberry). Native Rust implementation — no Python shell-out.

## Import

```niao
import "ngraphql"
```

Paths `import "std/ngraphql"` and `import "ngraphql"` are equivalent.

## Quick start

```niao
import "ngraphql"

let sdl = "
type Query {
  hello: String!
  hero(id: ID): Character
}
type Character {
  id: ID!
  name: String!
  friends: [Character!]
}
"

let schema = ngraphql.parse_schema(sdl)
let root = {
  hello: "world",
  hero: [
    {id: "1", name: "Luke", friends: [{id: "2", name: "Leia", friends: []}]},
    {id: "2", name: "Leia", friends: []}
  ]
}

let result = ngraphql.execute(schema, '
  query ($id: ID) {
    hello
    hero(id: $id) { name friends { name } }
  }
', root, {id: "1"})

print(result.data.hello)           // world
print(result.data.hero.name)       // Luke

let body = ngraphql.request_json("{ hello }")
print(body)                        // {"query":"{ hello }"}

ngraphql.close_schema(schema)
```

## Document handles

Parse executable GraphQL into opaque int handles; call `close_doc` when done.

| Method | Description |
|--------|-------------|
| `ngraphql.parse(source)` | Parse a query/mutation/subscription document. |
| `ngraphql.print(doc)` | Pretty-print a document. |
| `ngraphql.minify(source)` | Parse + compact print. |
| `ngraphql.gql(source)` | Canonicalize like the `gql` tag (parse + print). |
| `ngraphql.close_doc(doc)` | Free document handle. |
| `ngraphql.operations(doc)` | Array of `{name, kind, variables}`. |
| `ngraphql.fragments(doc)` | Array of `{name, type_condition}`. |
| `ngraphql.operation_names(doc)` | Named operation names. |
| `ngraphql.has_operation(doc, name)` | `true` when named operation exists. |
| `ngraphql.get_operation(doc, name?)` | Operation summary (name optional if single). |
| `ngraphql.variable_names(doc, operation?)` | Declared `$variable` names. |
| `ngraphql.spread_fragments(doc)` | Inline fragment spreads → new doc handle. |

## Client helpers

| Method | Description |
|--------|-------------|
| `ngraphql.request(query, variables?, operation_name?)` | `{query, variables?, operationName?}` object. |
| `ngraphql.request_json(query, variables?, operation_name?)` | Same as JSON string (HTTP POST body). |
| `ngraphql.is_document(source)` | `true` when source parses as a document. |
| `ngraphql.is_schema(source)` | `true` when source parses as SDL. |

## Schema handles

Parse GraphQL SDL. Built-in scalars (`Int`, `Float`, `String`, `Boolean`, `ID`) are always present. A type named `Query` is treated as the query root when no `schema { ... }` block is given.

| Method | Description |
|--------|-------------|
| `ngraphql.parse_schema(sdl)` | Parse SDL → schema handle. |
| `ngraphql.print_schema(schema)` | Print SDL text. |
| `ngraphql.close_schema(schema)` | Free schema handle. |
| `ngraphql.type_names(schema)` | Sorted type names (incl. builtins). |
| `ngraphql.describe_type(schema, name)` | `{kind, name, fields?, ...}` metadata. |
| `ngraphql.query_type(schema)` | Query root type name. |
| `ngraphql.mutation_type(schema)` | Mutation root or `nil`. |
| `ngraphql.subscription_type(schema)` | Subscription root or `nil`. |
| `ngraphql.has_type(schema, name)` | Type existence check. |

## Validate + execute

| Method | Description |
|--------|-------------|
| `ngraphql.validate(doc_or_query, schema, operation_name?)` | `{ok, errors:[{message, path}]}`. |
| `ngraphql.execute(schema, query, root?, variables?, operation_name?)` | `{data, errors?}`. |
| `ngraphql.execute_doc(schema, doc, root?, variables?, operation_name?)` | Same, using a parsed doc. |

Execution resolves fields by looking up property names on the root object (and nested objects). `@skip` / `@include` directives honor Boolean variables. An `id` argument on a list field filters matching objects.

## Errors

| Code | Meaning |
|------|---------|
| 4100 | Wrong argument count. |
| 4101 | General GraphQL error (catchable `ngraphql_error`). |
| 4102 | Type mismatch (hard error). |
| 4103 | Document / SDL parse failure. |
| 4104 | Invalid or closed document/schema handle. |

## Deferred / limitations

- No HTTP transport (compose with `http` / `nreq`).
- No subscription runtime / websocket push.
- No custom resolver callbacks (map-based property resolution only).
- Not a full GraphQL-spec validation suite (field existence + fragment checks).
- No federation / schema stitching.

## Example

See `examples/ngraphql_test.niao` and `examples/ngraphql_bench.niao`.
