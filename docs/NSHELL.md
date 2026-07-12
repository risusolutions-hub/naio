# nshell standard library

Subprocess execution with captured stdout/stderr, optional shell wrapping, timeouts, environment overrides, and PATH lookup.

Uses `std::process::Command` only — no external dependencies.

## Import

```niao
import "nshell"
```

Paths `import "std/nshell"` and `import "nshell"` are equivalent. Flat builtins (`nshell_run`, `nshell_which`, …) are also available globally after import.

## Quick start

```niao
import "nshell"

let r = nshell.run("echo hello", {shell: true})
print(r.stdout)   // hello
print(r.code)     // 0
print(r.ok)       // true

let out = nshell.run_capture(["niao", "--version"])
print(out)

print(nshell.which("niao"))   // path string or nil
print(nshell.exists("niao"))  // true / false
```

## Functions

| Method | Description |
|--------|-------------|
| `nshell.run(cmd, opts?)` | Run a command and return `{stdout, stderr, code, ok}`. `cmd` is a string (program or shell line) or an argv array (`["prog", "arg1", …]`). `ok` is `true` when exit code is 0. Spawn/timeout failures return a catchable `error`. |
| `nshell.run_capture(cmd)` | Shorthand for `nshell.run(cmd).stdout` (default options). |
| `nshell.which(name)` | Resolve `name` on `PATH`. Returns a path string or `nil`. |
| `nshell.exists(name)` | `true` when `nshell.which(name)` would return a path. |

## Options (`nshell.run`)

| Field | Type | Description |
|-------|------|-------------|
| `cwd` | string | Working directory for the child process. |
| `env` | object | Extra environment variables (`string` values). Merged over the current process environment. |
| `timeout_ms` | int | Kill wait after this many milliseconds; returns catchable `error` on timeout. `0` means no limit. |
| `shell` | bool | When `true` and `cmd` is a string, run through the system shell (`cmd /C` on Windows, `sh -c` elsewhere). Default `false`. |

### Command forms

- **String + `shell: false`** — `cmd` is the executable name (no arguments).
- **String + `shell: true`** — `cmd` is a full shell command line (e.g. `"echo hello"`).
- **Array** — `["program", "arg1", "arg2", …]`; shell is not used.

## Errors

| Code | Meaning |
|------|---------|
| 2930 | Wrong argument count. |
| 2931 | Spawn failure, I/O error, or timeout (catchable `nshell_error`). |
| 2932 | Invalid `cmd` type, empty argv array, or bad `opts` field types. |

## Notes

- Output is decoded as UTF-8 with lossy replacement for invalid bytes.
- On Windows, child processes are created with `CREATE_NO_WINDOW` so console tools do not flash a window.
- `echo` is a shell builtin on Windows; use `{shell: true}` or `which("cmd")` rather than expecting `which("echo")` to succeed.
