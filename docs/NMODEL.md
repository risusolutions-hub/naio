# nmodel — Prisma-style ORM

`nmodel` is a lightweight, zero-dependency ORM for Niao that works on top of
existing **nsqlite** and **npg** connection handles.

## Import

```niao
import "nmodel"
import "nsqlite"   // or "npg" for PostgreSQL
```

---

## Quick start

```niao
fn main() {
    // 1. Open a database (nsqlite in-memory here)
    let db = nsqlite.open(":memory:")

    // 2. Declare the schema
    let s = nmodel.schema({
        models: {
            User: {
                fields: {
                    id:    "int@id",
                    name:  "string@required",
                    email: "string@unique"
                }
            }
        }
    })

    // 3. Bind schema to the connection (dialect defaults to "sqlite")
    let c = nmodel.bind(s, db)

    // 4. Auto-migrate (CREATE TABLE if not exists)
    nmodel.migrate(c)

    // 5. CRUD
    let user = nmodel.create(c, "User", {name: "Niao", email: "niao@niao.dev"})
    print(user.id)   // → 1

    let rows = nmodel.find_many(c, "User", {limit: 10})
    let row  = nmodel.find_unique(c, "User", {where: {id: 1}})

    nmodel.update(c, "User", {where: {id: 1}, data: {name: "Niao 2"}})
    nmodel.delete(c, "User", {where: {id: 1}})

    let count = nmodel.raw(c, "SELECT count(*) AS n FROM \"User\"")[0].n
}
```

---

## Schema DSL

### Field spec: `"type[@attr1[@attr2...]]"`

| Type | SQL (SQLite) | SQL (Pg) |
|---|---|---|
| `int` | `INTEGER` | `INTEGER` |
| `float` | `REAL` | `REAL` |
| `string` | `TEXT` | `TEXT` |
| `bool` | `INTEGER` | `BOOLEAN` |
| `datetime` | `TEXT` | `TIMESTAMPTZ` |

### Attributes

| Attribute | Effect |
|---|---|
| `@id` | `PRIMARY KEY AUTOINCREMENT` / `GENERATED ALWAYS AS IDENTITY` |
| `@unique` | Adds `UNIQUE` constraint |
| `@required` | Adds `NOT NULL` |
| `@default(val)` | Adds `DEFAULT val` (number/SQL keyword kept bare; text quoted) |

### Examples

```niao
fields: {
    id:         "int@id",
    email:      "string@unique@required",
    score:      "float@default(0)",
    active:     "bool@default(TRUE)",
    created_at: "datetime@default(CURRENT_TIMESTAMP)"
}
```

---

## API Reference

### `nmodel.schema(spec) → schema_id`

Parse a schema object into a registered schema handle.

```niao
let s = nmodel.schema({
    models: {
        Post: { fields: {id: "int@id", title: "string@required", body: "string"} }
    }
})
```

---

### `nmodel.bind(schema_id, db_handle, dialect?) → client_id`

Bind a schema to a database connection. `dialect` defaults to `"sqlite"`.
Accepted values: `"sqlite"`, `"pg"` / `"postgres"`.

```niao
let c = nmodel.bind(s, db)            // SQLite
let c = nmodel.bind(s, pg_conn, "pg") // PostgreSQL
```

---

### `nmodel.migrate(client_id) → applied_count`

Ensure all model tables exist. Tracks applied tables in `_nmodel_migrations`.
Only creates tables that haven't been migrated yet.

```niao
let applied = nmodel.migrate(c)
print("tables created: " + applied)
```

---

### `nmodel.create(client_id, model_name, data{}) → row`

Insert a row and return the created object.

```niao
let post = nmodel.create(c, "Post", {title: "Hello", body: "World"})
print(post.id)
```

---

### `nmodel.find_many(client_id, model_name, opts?) → rows[]`

Select multiple rows. `opts` supports:
- `where: {col: val, ...}` — equality filters (ANDed)
- `limit: int`
- `order: "col ASC"` / `"col DESC"`

```niao
let posts = nmodel.find_many(c, "Post", {where: {title: "Hello"}, limit: 5})
let all   = nmodel.find_many(c, "Post")
```

---

### `nmodel.find_unique(client_id, model_name, {where: {...}}) → row | nil`

Return one row or `nil`.

```niao
let post = nmodel.find_unique(c, "Post", {where: {id: 1}})
if post == nil {
    print("not found")
}
```

---

### `nmodel.update(client_id, model_name, {where: {...}, data: {...}}) → row`

Update matching rows and return the first updated row.

```niao
let updated = nmodel.update(c, "Post", {
    where: {id: 1},
    data:  {title: "Updated title"}
})
```

---

### `nmodel.delete(client_id, model_name, {where: {...}}) → count`

Delete matching rows, return deleted count.

```niao
let n = nmodel.delete(c, "Post", {where: {id: 1}})
```

---

### `nmodel.raw(client_id, sql, params?) → rows[] | int`

Execute raw SQL. Returns rows array for `SELECT`, affected count for DML.

```niao
let rows = nmodel.raw(c, "SELECT * FROM \"Post\" WHERE id > ?", [0])
let n    = nmodel.raw(c, "DELETE FROM \"Post\" WHERE id = ?", [42])
```

---

### `nmodel.schema_info(schema_id) → {ModelName: fields[]}`

Introspect a registered schema. Each field object has keys:
`name`, `type`, `is_id`, `is_unique`, `nullable`, `default`.

---

## Migration table

`nmodel` tracks applied migrations in `_nmodel_migrations`:

```sql
CREATE TABLE "_nmodel_migrations" (
  model_name TEXT PRIMARY KEY NOT NULL,
  applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
)
```

---

## Error codes

| Code | Constant | Meaning |
|---|---|---|
| 2830 | `E2830_NMODEL_ARITY` | Wrong argument count |
| 2831 | `E2831_NMODEL_ERROR` | Query/migration failure |
| 2832 | `E2832_NMODEL_TYPE` | Type mismatch in argument |
| 2833 | `E2833_NMODEL_SCHEMA` | Schema validation error |

---

## Wiring snippets (for orchestrator)

Add to `crates/niao_runtime/src/lib.rs`:

```rust
// ── in module declarations ──
mod nmodel;

// ── in builtins() fn ──
builtins.extend(nmodel::builtins());

// ── in setup_env() / namespace registration ──
env.define(nmodel::MODULE_NAME.to_string(), nmodel::namespace().ref_cell());

// ── in resolve_module_name() ──
if nmodel::MODULE_PATHS.contains(&path) {
    return Some(nmodel::MODULE_NAME);
}
```
