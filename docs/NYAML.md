# nyaml — YAML 1.2 parse & emit

YAML 1.2 parse and emit with safe-by-default loading, anchor/alias support, and multi-document streams. ~PyYAML / ruamel.yaml subset.

## Import

```niao
import "nyaml"
```

Paths `import "std/nyaml"` and `import "nyaml"` are equivalent. Flat builtins (`nyaml_parse`, `nyaml_emit`, …) are also available globally after import.

## Quick start

```niao
import "nyaml"

// Parse a config file (safe by default — rejects !!python/* tags)
let cfg = nyaml.parse_file("config.yaml")
print(cfg.app.name)

// Parse inline
let doc = nyaml.parse("items:\n  - a\n  - b\n")
print(len(doc.items))   // 2

// Multi-document stream
let docs = nyaml.parse_all("---\n{a: 1}\n---\n{b: 2}\n")
print(len(docs))          // 2

// Emit
let text = nyaml.emit({version: 1, enabled: true})
let pretty = nyaml.emit_pretty(doc, {indent: 2})

// PyYAML-style aliases
let loaded = nyaml.load("key: value\n")
let dumped = nyaml.dump(loaded)
```

## Functions

| Method | Description |
|--------|-------------|
| `nyaml.parse(text, opts?)` | Parse a single YAML document. Default `opts.safe: true`. Errors on multi-doc unless `opts.multi: true`. |
| `nyaml.parse_all(text, opts?)` | Parse every `---`-delimited document; returns an array. |
| `nyaml.safe_parse(text)` | Shorthand for `parse(text)` with safe mode (default). |
| `nyaml.safe_parse_all(text)` | Shorthand for safe multi-document parse. |
| `nyaml.parse_file(path, opts?)` | Read a file and parse the first document. |
| `nyaml.emit(value, opts?)` | Serialize a Niao value to YAML text. |
| `nyaml.emit_pretty(value, opts?)` | Block-style emit; `opts.indent` (default 2). |
| `nyaml.emit_all(values, opts?)` | Serialize an array of values as a multi-doc stream. |
| `nyaml.emit_file(path, value, opts?)` | Write YAML to a file; returns `true`. |
| `nyaml.valid(text)` | `true` when `text` is syntactically valid YAML. |
| `nyaml.load(text, opts?)` | Alias for `parse` (PyYAML compat). |
| `nyaml.dump(value, opts?)` | Alias for `emit` (PyYAML compat). |

### Parse options

| Key | Default | Description |
|-----|---------|-------------|
| `safe` | `true` | Reject non-standard tags (`!!python/object`, custom `!tags`). |
| `multi` | `false` | Allow multiple documents in `parse()` (otherwise use `parse_all`). |

### Emit options

| Key | Default | Description |
|-----|---------|-------------|
| `flow` | auto | `true` for flow style (`[a, b]`), `false` for block. |
| `indent` | `2` | Block indentation width. |
| `width` | `80` | Preferred line width (hint). |
| `sort_keys` | `false` | Sort mapping keys lexicographically. |
| `explicit_start` | `false` | Prefix with `---`. |
| `explicit_end` | `false` | Suffix with `...`. |
| `unicode` | `true` | Emit UTF-8 literally; `false` escapes non-ASCII. |

### Value mapping

| YAML | Niao |
|------|------|
| `null` | `nil` |
| `true` / `false` | `bool` |
| integers | `int` (or `bigint` when out of range) |
| floats | `float` |
| strings | `string` |
| sequences | `array` |
| mappings | `object` |
| tagged nodes | `{__tag: "!!timestamp", value: ...}` |

Anchors (`&id`) and aliases (`*id`, merge `<<:`) are resolved on parse (same semantics as PyYAML `safe_load`).

## Size limits

Inputs and outputs are capped at **64 MiB** per operation.

## Errors

| Code | Meaning |
|------|---------|
| 4300 | Wrong argument count. |
| 4301 | I/O or emit failure (catchable `nyaml_error`). |
| 4302 | Wrong argument type (hard error). |
| 4303 | Parse failure, unsafe tag, or multi-doc in `parse()` (catchable `nyaml_error`). |

## Deferred / limitations

- **Round-trip anchor preservation** on emit (ruamel.yaml-style) is not implemented; anchors are resolved on load and re-emitted without `&`/`*` aliases unless the emitter deduplicates structures.
- **Custom YAML constructors** (unsafe `load()` with arbitrary tags) are intentionally unsupported; use `safe: false` only for known custom tags you trust.
- **JSON schema validation** of YAML content is out of scope — pair with `nvalid` or `nschema`.

## See also

- `json` — JSON parse/stringify.
- `ntoml` — TOML configuration files.
- `nencoding` — charset detection for legacy YAML files not in UTF-8.
