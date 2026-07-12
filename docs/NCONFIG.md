# nconfig standard library

Layered configuration with precedence **defaults → file (json/toml) → env → args**, plus typed schema validation.

## Import

```niao
import "nconfig"
```

Paths `import "std/nconfig"` and `import "nconfig"` are equivalent. Flat builtins (`nconfig_new`, `nconfig_get`, …) are also available globally after import.

## Quick start

```niao
import "nconfig"

let cfg = nconfig.new({port: 8080, host: "localhost"})
nconfig.file(cfg, "config.json")
nconfig.env(cfg, "APP_")
nconfig.args(cfg)

nconfig.schema(cfg, {
    port: {type: "int", required: true, min: 1, max: 65535},
    host: {type: "string", required: true},
    debug: {type: "bool", default: false},
})
print(nconfig.validate(cfg))   // true

let all = nconfig.resolve(cfg)
print(all.port, all.host, all.debug)
nconfig.close(cfg)
```

## Layer order

Each layer **deep-merges** into the handle. Later layers override scalars; nested objects are merged recursively.

| Layer | Function | Notes |
|-------|----------|-------|
| Defaults | `nconfig.new(defaults?)` | Optional initial object. |
| File | `nconfig.file(h, path)` | `.json` / `.jsonc` / `.toml` by extension; unknown extension tries JSON then TOML. |
| Env | `nconfig.env(h, prefix?)` | `APP_DB_HOST` with prefix `APP_` → `db.host` (lowercase, `_` → `.`). Values are coerced (bool/int/float/JSON). |
| Args | `nconfig.args(h, argv?)` | `--port=9000`, `--verbose` (bool), `-x` short flags; positionals stored in `_args`. Uses `std::env::args()` when `argv` omitted. |

## Functions

| Method | Description |
|--------|-------------|
| `nconfig.new(defaults?)` | Create a config handle (`int`). Returns catchable error on bad types. |
| `nconfig.file(h, path)` | Merge a JSON or TOML file. Hard error if unreadable; catchable error on invalid handle. |
| `nconfig.env(h, prefix?)` | Merge environment variables. |
| `nconfig.args(h, argv?)` | Merge CLI args (array of strings) or process argv. |
| `nconfig.schema(h, schema)` | Attach a typed schema object for `validate` / `resolve`. |
| `nconfig.validate(h)` | Apply defaults, check types and required fields. Returns `true` or catchable `nconfig_error`. |
| `nconfig.get(h, key?)` | Dot-path lookup (`"db.host"`) or full object when `key` omitted. Missing key → catchable error (3163). |
| `nconfig.resolve(h)` | Full merged object with schema defaults applied. |
| `nconfig.close(h)` | Drop handle; returns whether it existed. |

## Schema

Schema keys map to top-level config fields. Each rule is an object:

| Field | Meaning |
|-------|---------|
| `type` | `string`, `int`, `float`, `number`, `bool`, `array`, `object`, `any` |
| `required` | `true` when the field must be present after layers |
| `default` | Value applied by `validate` / `resolve` when missing |
| `min` / `max` | Numeric bounds |
| `items` | Element rule for `array` |
| `properties` | Nested object rules |

## Errors

| Code | Meaning |
|------|---------|
| 3160 | Wrong argument count. |
| 3161 | Operation error (invalid handle, parse failure, validation failure) — catchable `nconfig_error`. |
| 3162 | Wrong argument type. |
| 3163 | Missing required config key — catchable `nconfig_error`. |
