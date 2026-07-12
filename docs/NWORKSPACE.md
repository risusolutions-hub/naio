# nworkspace standard library

Workspace manifest loading, member dependency graph, topological ordering, and subprocess `run` for member entry files.

## Import

```niao
import "nworkspace"
```

Paths `import "std/nworkspace"` and `import "nworkspace"` are equivalent.

## Manifest format (`niao.workspace.toml`)

```toml
name = "demo-workspace"

[[members]]
name = "core"
path = "packages/core"
entry = "main.niao"
depends = []

[[members]]
name = "app"
path = "packages/app"
entry = "main.niao"
depends = ["core"]
```

- `path` is relative to the manifest directory (for `load`) or process cwd (for `parse`).
- `entry` defaults to `main.niao`.
- `depends` lists other member names (must exist); used for topological sort.

## Quick start

```niao
import "nworkspace"

let ws = nworkspace.load("niao.workspace.toml")
print(nworkspace.order(ws))        // ["core", "app"]
print(nworkspace.graph(ws))        // {core: [], app: ["core"]}
print(nworkspace.member_path(ws, "app"))

let result = nworkspace.run(ws, "core")   // {stdout, stderr, code, ok}
print(result.ok, result.stdout)
```

## Functions

| Method | Description |
|--------|-------------|
| `nworkspace.load(path)` | Read TOML manifest; return workspace object. |
| `nworkspace.parse(text)` | Parse manifest string (root = cwd). |
| `nworkspace.members(ws)` | Member info array. |
| `nworkspace.graph(ws)` | Adjacency object `{name: [deps…]}`. |
| `nworkspace.order(ws)` | Topologically sorted member names; cycle → catchable error. |
| `nworkspace.member_path(ws, name)` | Resolved member root path. |
| `nworkspace.run(ws, name, mode?)` | `niao run <entry>` in member dir; optional `"interp"` / `"vm"`. |

Workspace objects include `name`, `root`, `manifest`, and `members`.

## Errors

| Code | Meaning |
|------|---------|
| 3230 | Wrong argument count. |
| 3231 | I/O, parse, unknown member, cycle, missing entry, spawn failure (catchable). |
| 3232 | Wrong argument type. |
