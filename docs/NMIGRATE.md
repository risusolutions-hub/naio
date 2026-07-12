# nmigrate standard library

Schema diff from nmodel-style struct definitions to SQL migration statements for **SQLite** and **PostgreSQL**. Complements `nmodel.migrate()` (create-path only) with incremental ALTER / DROP planning.

## Import

```niao
import "nmigrate"
```

Paths `import "std/nmigrate"` and `import "nmigrate"` are equivalent. Flat builtins (`nmigrate_diff`, `nmigrate_plan`, …) are also available globally after import.

## Quick start

```niao
import "nmigrate"

let v1 = {
    models: {
        User: {
            fields: {
                id:   "int@id",
                name: "string@required"
            }
        }
    }
}

let v2 = {
    models: {
        User: {
            fields: {
                id:    "int@id",
                name:  "string@required",
                email: "string@unique"
            }
        },
        Post: {
            fields: {
                id:    "int@id",
                title: "string@required"
            }
        }
    }
}

let plan = nmigrate.plan(v1, v2, "sqlite")
for sql in plan.sql { print(sql) }
print(plan.summary)   // {create_table: 1, add_column: 1, ...}
```

## Schema format

Uses the same DSL as `nmodel.schema()` — see [NMODEL.md](NMODEL.md). Field specs: `"type[@attr…]"` with types `int`, `float`, `string`, `bool`, `datetime` and attrs `@id`, `@unique`, `@required`, `@default(val)`.

## Functions

| Method | Description |
|--------|-------------|
| `nmigrate.diff(old, new)` | `{changes, summary}` — structured change list without SQL. |
| `nmigrate.sql(old, new, dialect?)` | Array of SQL strings. Dialect defaults to `"sqlite"`; use `"pg"` / `"postgres"`. |
| `nmigrate.plan(old, new, dialect?)` | `{changes, summary, sql}` — diff + SQL together. |
| `nmigrate.dialect(name)` | Normalize dialect string (`sqlite` or `pg`). |

### Change kinds

| `kind` | Meaning |
|--------|---------|
| `create_table` | New model in target schema → `CREATE TABLE IF NOT EXISTS`. |
| `drop_table` | Model removed → `DROP TABLE IF EXISTS`. |
| `add_column` | New field on existing table → `ALTER TABLE … ADD COLUMN`. |
| `drop_column` | Field removed (never drops `@id`) → `ALTER TABLE … DROP COLUMN`. |
| `alter_column` | Type change → `ALTER COLUMN … TYPE` on PostgreSQL; SQLite emits a `-- manual migration` comment. |

## Errors

| Code | Meaning |
|------|---------|
| 3260 | Wrong argument count. |
| 3261 | Invalid dialect or schema semantic error. |
| 3262 | Wrong argument type. |
