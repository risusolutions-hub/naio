# nmysql standard library

MySQL / MariaDB client for Niao: connection pools, prepared statements, transactions, schema migrations, introspection, and async background queries. Implemented in Rust via the `mysql` crate (pure Rust wire protocol) with `niao_db` pooling. Completes the SQL big-4 next to `npg` / `nsqlite` / `nmongo` (~pymysql, mysqlclient).

## Import

```niao
import "nmysql"
```

Use the **`nmysql`** namespace for short names:

```niao
let db = nmysql.connect("mysql://user:pass@localhost:3306/mydb")
nmysql.migrate(db, [{version: 1, sql: "CREATE TABLE t (id INT AUTO_INCREMENT PRIMARY KEY)"}])
```

Paths `import "std/nmysql"` and `import "nmysql"` are equivalent.

Flat builtins (`nmysql_connect`, `nmysql_query`, …) are also available globally after import.

## Connection

| Method / Builtin | Description |
|------------------|-------------|
| `nmysql.connect(url)` | Connect via `mysql://` URL |
| `nmysql.connect_opts(opts)` | Object: `host`, `port`, `user`, `password`, `database`, `sslmode`, `connect_timeout`, or `url` |
| `nmysql.close(conn)` | Close connection and invalidate statement handles |
| `nmysql.ping(conn)` | `SELECT 1` health check |
| `nmysql.conninfo(conn)` | Redacted connection info (password hidden) |
| `nmysql.configure(conn, opts)` | Session: `max_execution_time`, `wait_timeout`, `time_zone`, `charset` |
| `nmysql.server_version(conn)` | `SELECT VERSION()` string |
| `nmysql.is_in_transaction(conn)` | Bool (client-tracked transaction state) |
| `nmysql.last_insert_id(conn)` | Last `AUTO_INCREMENT` id on this connection |
| `nmysql.affected_rows(conn)` | Rows affected by last DML |

**SSL modes:** v0.1 supports `sslmode=disable` only. Other modes return `E1917`.

## Connection pool

| Method | Description |
|--------|-------------|
| `nmysql.pool(opts)` | Create pool; opts include URL or discrete fields + `max_size`, `min_idle`, `max_lifetime_secs`, `connection_timeout_secs` |
| `nmysql.pool_close(pool)` | Drain and close pool |
| `nmysql.pool_get(pool)` | Checkout connection handle |
| `nmysql.pool_status(pool)` | `{size, idle, in_use}` |

## Schema & migrations

| Method | Description |
|--------|-------------|
| `nmysql.exec(conn, sql, params?)` | DDL/DML without result set; returns affected row count |
| `nmysql.exec_many(conn, sql_list)` | Multiple statements in one transaction |
| `nmysql.migrate(conn, migrations)` | Apply `{version, sql}` objects in order; tracks `_nmysql_schema_version` |
| `nmysql.table_exists(conn, name)` / `(conn, schema, name)` | `information_schema` lookup (default: current database) |
| `nmysql.list_tables(conn, schema?)` | Table names |
| `nmysql.table_info(conn, table, schema?)` | Column metadata: `name`, `type`, `nullable`, `default` |
| `nmysql.list_indexes(conn, schema?, table?)` | Index metadata |

## Queries

| Method | Description |
|--------|-------------|
| `nmysql.query(conn, sql, params?, format?)` | All rows; `format` is `"object"` (default) or `"array"` |
| `nmysql.query_row(conn, sql, params?)` | First row object or `nil` |
| `nmysql.query_value(conn, sql, params?)` | Scalar |
| `nmysql.query_column(conn, sql, params?)` | First column of all rows |

**Placeholders:** MySQL-native `?`. You may also use `$1`, `$2`, …; they are rewritten to `?` before execution.

## Prepared statements

| Method | Description |
|--------|-------------|
| `nmysql.prepare(conn, sql)` | Statement handle (validates SQL on the server) |
| `nmysql.bind(stmt, index, value)` | Positional bind (1-based) |
| `nmysql.stmt_exec(stmt)` | Execute without rows |
| `nmysql.stmt_query(stmt, format?)` | Execute with rows |
| `nmysql.stmt_reset(stmt)` | Clear bindings |
| `nmysql.finalize(stmt)` | Free statement |

## Transactions

| Method | Description |
|--------|-------------|
| `nmysql.begin(conn, opts?)` | `isolation`, `read_only` |
| `nmysql.commit(conn)` | Commit |
| `nmysql.rollback(conn)` | Rollback |
| `nmysql.savepoint(conn, name)` | `SAVEPOINT` |
| `nmysql.rollback_to(conn, name)` | `ROLLBACK TO SAVEPOINT` |

## Batch & helpers

| Method | Description |
|--------|-------------|
| `nmysql.batch(conn, sql, rows)` | Repeated exec in one transaction |
| `nmysql.insert(conn, table, data, schema?)` | Insert from object; returns `{last_insert_id, affected_rows}` |
| `nmysql.version()` | Library version string |
| `nmysql.escape_literal(s)` | Quoted SQL string literal |
| `nmysql.quote_ident(s)` | Backtick-quoted identifier |

## Async

| Method | Description |
|--------|-------------|
| `nmysql.async_exec(conn, sql, params?)` | Background exec; returns task id |
| `nmysql.async_query(conn, sql, params?, format?)` | Background query; returns task id |
| `nmysql.task_done(task)` | Bool |
| `nmysql.task_wait(task)` | Block until finished |
| `nmysql.task_result(task)` | Result value |
| `nmysql.task_cancel(task)` | Request cancel |

Async workers reopen a fresh connection from the stored URL (they do not share the live handle).

## Quickstart

```niao
import "nmysql"
import "nenv"

fn main() {
    let url = nenv.get("NIAO_MYSQL_URL")
    if url == nil || url == "" {
        print("set NIAO_MYSQL_URL to run this demo")
        return
    }
    let db = nmysql.connect(url)
    nmysql.exec(db, "CREATE TABLE IF NOT EXISTS greet (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(64))")
    let ins = nmysql.insert(db, "greet", {name: "Niao"})
    print(ins.last_insert_id)
    let rows = nmysql.query(db, "SELECT id, name FROM greet WHERE id = ?", [ins.last_insert_id])
    print(rows[0].name)
    nmysql.close(db)
}
```

## Tests

Set `NIAO_MYSQL_URL` (for example `mysql://root:pass@127.0.0.1:3306/test`) and run:

```
niao run tests/nmysql.niao
```

Without the env var the test skips cleanly.

## Error codes

| Code | Kind | Meaning |
|------|------|---------|
| E1910 | `nmysql_error` | Arity / argument shape |
| E1911 | `nmysql_error` | MySQL operation failed |
| E1912 | `nmysql_error` | Invalid or closed handle |
| E1913 | `nmysql_error` | Schema / constraint |
| E1914 | `nmysql_error` | Migration error |
| E1915 | `nmysql_error` | Async task not found |
| E1916 | `nmysql_error` | Invalid bind value |
| E1917 | `nmysql_error` | TLS / connection error |
