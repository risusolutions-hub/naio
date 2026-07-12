# Niao Stdlib Expansion Batch 2 — 10 New Libraries

Goal: 10 fast, lightweight, std-only native modules in `crates/niao_runtime`.
Each subagent creates ONLY its module file + docs + example + niao_libs package.
Central integration (codes.rs, lib.rs, catalog) done after all modules land.

## Error code map (2840–2939)

```
ntoml     2840 arity, 2841 error, 2842 type, 2843 parse
ncsv      2850 arity, 2851 error, 2852 type, 2853 parse
nmarkdown 2860 arity, 2861 error, 2862 type
nws       2870 arity, 2871 error, 2872 type, 2873 invalid handle
nurl      2880 arity, 2881 error, 2882 type
nsmtp     2890 arity, 2891 error, 2892 type
nsemver   2900 arity, 2901 error, 2902 parse
ncron     2910 arity, 2911 error, 2912 parse
nprompt   2920 arity, 2921 error, 2922 type
nshell    2930 arity, 2931 error, 2932 type
```

## Integration pattern (follow nvalid.rs exactly)

- Flat builtins: `<lib>_<fn>` + namespace short names
- `MODULE_NAME`, `MODULE_PATHS`, `builtins()`, `namespace()`
- Hard errors → `RuntimeError::at(span, code, ...)`
- Recoverable → `error_value(code, "<lib>_error", msg, span)`
- `#[cfg(test)]` unit tests in module file
- DO NOT modify lib.rs / codes.rs / catalog — parent agent wires up

## Reuse existing internals where noted

- **ntoml**: `niao_json_core::toml::parse` / stringify via existing Value bridge
- **ncsv**: standalone lightweight CSV (ncl has DataFrame CSV; ncsv is simple row arrays)
- **nws**: delegate to `net::websocket` handle registry (same handles)
- **nsmtp**: delegate to `net::smtp::net_smtp_send` logic or reimplement thin wrapper
- **nurl**: hand-rolled parse/build (no url crate)
