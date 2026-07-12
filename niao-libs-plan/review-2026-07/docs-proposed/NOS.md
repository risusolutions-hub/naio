# nos standard library

Operating-system interface: process info, platform constants, environment, a subprocess runner, and
lightweight filesystem helpers. Python-`os`-flavored surface.

## Import

```niao
import "nos"
```

`import "std/nos"` and `import "nos"` are equivalent.

## Quick start

```niao
import "nos"

print(nos.platform())        // "windows" | "linux" | "macos"
print(nos.arch())            // "x86_64" | "aarch64" | ...
print(nos.cpu_count())       // logical CPUs
print(nos.hostname())
print(nos.username())

let code = nos.system("echo hi")   // run a shell command, returns exit code
```

## Process & platform

| Method | Description |
|--------|-------------|
| `nos.platform()` | OS name. |
| `nos.arch()` | CPU architecture. |
| `nos.cpu_count()` | Logical CPU count. |
| `nos.hostname()` · `nos.username()` | Machine / user. |
| `nos.getpid()` · `nos.getppid()` | Process / parent PID. |
| `nos.argv()` | Process arguments. |
| `nos.system(cmd)` | Run a command via the shell; returns exit code. |
| `nos.exit(code)` | Exit the process. |

## Filesystem

| Method | Description |
|--------|-------------|
| `nos.getcwd()` · `nos.chdir(path)` | Working directory. |
| `nos.exists(path)` · `nos.isfile(path)` · `nos.isdir(path)` | Existence / type tests. |
| `nos.listdir(path)` | Directory entries. |
| `nos.mkdir(path)` · `nos.makedirs(path)` | Create dir / dirs. |
| `nos.remove(path)` · `nos.rmdir(path)` | Delete file / empty dir. |
| `nos.rename(from, to)` | Move/rename. |
| `nos.stat(path)` · `nos.lstat(path)` | Metadata `{size, modified, created, accessed, ...}`. |

## Notes & v0.2.4

- Overlaps `io` on path/dir helpers — `nos` is the "process + POSIX-ish" front, `io` is the
  streaming/file-content front. Cross-link in docs so users pick correctly.
- Planned: `nos.spawn(cmd, {env, cwd})` with captured stdout/stderr, `nos.which(name)`,
  `nos.getenv`/`setenv` (or defer to `nenv`), `nos.glob(pattern)`, signal handling.

> **Status:** drafted from `crates/niao_runtime/src/nos.rs` (24 builtins). Verify against source.
