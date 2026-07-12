# nscaffold standard library

Generate CRUD HTTP routes, `nmodel` schema objects, SQL `CREATE TABLE` migrations, and `ntest` stubs from a struct spec.

## Import

```niao
import "nscaffold"
```

Paths `import "std/nscaffold"` and `import "nscaffold"` are equivalent.

## Quick start

```niao
import "nscaffold"

let spec = {
    name: "User",
    fields: {
        id:    "int@id",
        name:  "string@required",
        email: "string@unique@required"
    }
}

let bundle = nscaffold.crud(spec)
print(bundle.table)       // users
print(bundle.path)        // /users
print(bundle.migration)   // CREATE TABLE ...
// bundle.routes, bundle.model, bundle.tests are ready to paste
```

## Spec object

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Model / struct name (PascalCase), e.g. `User`. |
| `fields` | yes | Map of field name → nmodel field spec (`"int@id"`, `"string@required"`, …). |
| `table` | no | SQL table name (default: snake plural of `name`, e.g. `users`). |
| `path` | no | HTTP path prefix (default: `/<table>`). |

## Functions

| Method | Description |
|--------|-------------|
| `nscaffold.crud(spec)` | Full bundle: `{name, table, path, routes, model, migration, tests}`. |
| `nscaffold.routes(spec)` | Niao route blocks (GET list, GET by id, POST, PUT, DELETE). |
| `nscaffold.model(spec)` | `nmodel.schema`-compatible object. |
| `nscaffold.migration(spec)` | SQLite `CREATE TABLE` DDL (via nmodel field specs). |
| `nscaffold.tests(spec)` | `ntest` + `nmodel` + `nsqlite` scaffold source string. |

Invalid specs return catchable `nscaffold_error` values.

## Errors

| Code | Meaning |
|------|---------|
| 3250 | Wrong argument count. |
| 3251 | Scaffold / spec error (catchable). |
| 3252 | Type error. |
