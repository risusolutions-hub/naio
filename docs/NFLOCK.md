# nflock — advisory file locks, lockfiles & PID files

Advisory file locks, lockfiles, PID files, and timeouts (~Python `filelock` + `fcntl` subset). Uses `flock` on Unix, `LockFileEx` on Windows, and POSIX `fcntl` record locks via [`fs2`] when `use_flock: false`.

## Import

```niao
import "nflock"
```

Paths `import "std/nflock"` and `import "nflock"` are equivalent. Flat builtins (`nflock_lock`, `nflock_acquire`, …) are also available globally after import.

## Quick start

```niao
import "nflock"

// One-shot exclusive lock (~FileLock)
let lk = nflock.lock("/tmp/myapp.lock", {timeout_ms: 5000})
print(lk.path, lk.locked)
nflock.release(lk)
nflock.close(lk)

// PID file for single-instance daemons
let pf = nflock.pid_acquire("/var/run/myapp.pid")
print("running as pid", pf.pid)
nflock.pid_release(pf)
```

## Constants

| Name | Value | Meaning |
|------|-------|---------|
| `nflock.LOCK_SH` | 1 | Shared / read lock (`flock`). |
| `nflock.LOCK_EX` | 2 | Exclusive / write lock (`flock`). |
| `nflock.LOCK_NB` | 4 | Non-blocking flag (OR with `LOCK_SH`/`LOCK_EX`). |
| `nflock.LOCK_UN` | 8 | Unlock (`flock`). |
| `nflock.F_RDLCK` | 1 | Read record lock type (`lockf`). |
| `nflock.F_WRLCK` | 2 | Write record lock type (`lockf`). |
| `nflock.F_UNLCK` | 3 | Unlock record (`lockf`). |
| `nflock.F_GETLK` | 5 | Test record lock (`lockf`). |
| `nflock.F_SETLK` | 6 | Set record lock, non-blocking (`lockf`). |
| `nflock.F_SETLKW` | 7 | Set record lock, blocking (`lockf`). |

## Lockfile API (~filelock)

| Method | Description |
|--------|-------------|
| `nflock.open(path, opts?)` | Open lock file; returns `{handle, path, locked}`. |
| `nflock.file(path, opts?)` | Alias for `open`. |
| `nflock.lock(path, opts?)` | Open and acquire in one step; `locked` is `true`. |
| `nflock.acquire(handle, opts?)` | Block until lock acquired (or timeout). |
| `nflock.try_acquire(handle, opts?)` | Non-blocking acquire → `true`/`false`. |
| `nflock.release(handle)` | Release advisory lock; keeps file open. |
| `nflock.locked(handle)` | Whether handle currently holds a lock. |
| `nflock.close(handle)` | Drop handle (auto-releases on drop). |
| `nflock.path(handle)` | Lock file path string. |
| `nflock.info(handle)` | `{handle, path, locked, mode?}`. |
| `nflock.break_stale(path, force?)` | Remove lockfile when embedded PID is dead. |

### Lock options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `create` | bool | `true` | Create lock file if missing. |
| `mode` | string | `"exclusive"` | `"shared"` / `"exclusive"` (also `shared: true`). |
| `timeout_ms` | int | — | Max wait; omit for infinite block. `0` = try once. |
| `poll_ms` | int | `50` | Poll interval when waiting with timeout. |
| `use_flock` | bool | `true` | Use BSD `flock`; `false` uses POSIX `fcntl` via fs2. |
| `content` | string | — | Write after acquire (e.g. PID string). |

## Low-level flock / lockf

| Method | Description |
|--------|-------------|
| `nflock.flock(handle, op)` | BSD `flock` / Windows `LockFileEx` on open handle. |
| `nflock.lockf(handle, cmd, len?, start?)` | POSIX record lock (`fcntl`); `len`/`start` default `0` (whole file). |

## PID file API

| Method | Description |
|--------|-------------|
| `nflock.pid_acquire(path, opts?)` | Exclusive lock + write current PID; breaks stale locks. |
| `nflock.pid_read(path)` | Read PID integer from file. |
| `nflock.pid_write(path, pid?)` | Write PID (default: current process) without locking. |
| `nflock.pid_alive(pid)` | Whether process `pid` is running on this host. |
| `nflock.pid_remove(path)` | Delete PID file (no error if missing). |
| `nflock.pid_release(handle)` | Release lock and remove PID file. |

### PID options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `timeout_ms` | int | — | Max wait to acquire lock. |
| `poll_ms` | int | `50` | Poll interval during timeout wait. |
| `force` | bool | `false` | Break lock even when recorded PID is alive. |
| `write_pid` | bool | `true` | Write current PID after acquire. |

## Errors

| Code | Meaning |
|------|---------|
| `E3521` | Wrong argument count. |
| `E3522` | General `nflock_error` (I/O, already locked, live lock, …). |
| `E3523` | Type mismatch. |
| `E3524` | Invalid or closed handle. |
| `E3525` | Lock acquisition timeout. |

## Notes

- Locks are **advisory** — cooperating processes must use the same lock file.
- Handles auto-release locks when dropped / `close`d.
- `break_stale` reads an optional PID from the lockfile's first line; removes the file when that process is not alive.
- On Windows, `lockf`/`F_GETLK` are approximated via `LockFileEx` (no byte-range record locks).

## Benchmark

```bash
cargo run -p niao_flock --bin flock_bench --release
niao run examples/nflock_bench.niao
```
