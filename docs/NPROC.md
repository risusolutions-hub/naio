# nproc — child processes & IPC

Child processes beyond `nshell`: Popen-style streaming I/O, bounded process pools, anonymous OS pipes, in-process IPC channels/queues, file-backed shared memory, and sync primitives (~`multiprocessing` subset).

Uses the in-house `niao_proc` crate (OS `pipe`/`mmap` FFI, `std::process`, channels) — no third-party dependencies.

## Import

```niao
import "nproc"
```

Paths `import "std/nproc"` and `import "nproc"` are equivalent. Flat builtins (`nproc_spawn`, `nproc_pool_map`, …) are also available globally after import.

## Quick start

```niao
import "nproc"

// Popen-style child with streaming stdout
let p = nproc.spawn(["niao", "--version"])
print(p.pid)
print(nproc.stdout_read_all(p.handle))
print(nproc.wait(p.handle))

// Process pool — parallel argv batches
let pool = nproc.pool(4)
let cmds = [["niao", "--version"], ["niao", "--version"]]
let results = nproc.pool_map(pool, cmds)
for r in results { print(r.stdout, r.code, r.ok) }
nproc.pool_close(pool)

// Shared memory between processes
let shm = nproc.shared_memory("counter", 64)
nproc.shared_write(shm, 0, "hello")
print(nproc.shared_read(shm, 0, 5))
nproc.shared_unlink("counter")
```

## Process API

| Method | Description |
|--------|-------------|
| `nproc.cpu_count()` | Logical CPU count (≥ 1). |
| `nproc.active_count()` | Spawned processes still running in this VM. |
| `nproc.spawn(cmd, opts?)` | Start child; returns `{handle, pid}`. `cmd` is argv array or program string. |
| `nproc.poll(handle)` | Exit code when finished, else `nil`. |
| `nproc.wait(handle, timeout_ms?)` | Block until exit; optional timeout → catchable `nproc_error`. |
| `nproc.kill(handle)` / `terminate(handle)` | Force-terminate child. |
| `nproc.stdin_write(handle, data)` | Write bytes/string to stdin (requires `stdin_pipe: true`). |
| `nproc.stdout_read(handle, max?)` | Read up to `max` bytes from stdout (default 65536). |
| `nproc.stdout_read_all(handle)` | Drain stdout. |
| `nproc.stderr_read_all(handle)` | Drain stderr. |
| `nproc.communicate(handle, input?, timeout_ms?)` | Write optional input, close stdin, read all streams → `{stdout, stderr, code}`. |
| `nproc.close(handle)` | Drop process handle (does not kill running child). |

### Spawn options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `cwd` | string | — | Working directory. |
| `env` | object | — | Extra env vars (string values). |
| `stdin_pipe` | bool | `false` | Pipe stdin for `stdin_write`. |
| `stdout_pipe` | bool | `true` | Pipe stdout for reads. |
| `stderr_pipe` | bool | `true` | Pipe stderr for reads. |

## Process pool

| Method | Description |
|--------|-------------|
| `nproc.pool(workers)` | Create pool handle with max concurrent children. |
| `nproc.pool_map(pool, commands, opts?)` | Run each argv array; returns `[{stdout, stderr, code, ok}, …]` in order. |
| `nproc.pool_map_argv(pool, template, items, opts?)` | Append each string item to `template` argv. |
| `nproc.pool_close(pool)` | Mark pool closed (no new maps). |
| `nproc.pool_join(pool)` | Wait until in-flight pool jobs finish. |

## OS pipes

| Method | Description |
|--------|-------------|
| `nproc.pipe()` | Anonymous OS pipe → `{handle}`. |
| `nproc.pipe_read(handle, max?)` | Read bytes as string. |
| `nproc.pipe_write(handle, data)` | Write bytes/string. |
| `nproc.pipe_close_read(handle)` / `pipe_close_write(handle)` | Close one end. |

## Channels & queues (in-process)

Thread-safe value channels inside the Niao VM (same process). For cross-process messaging, use `spawn` + pipes or shared memory.

| Method | Description |
|--------|-------------|
| `nproc.channel(capacity?)` | Bounded channel (default unbounded when called with no args). |
| `nproc.channel_send(ch, value)` | Send a value; blocks when bounded and full. |
| `nproc.channel_recv(ch, timeout_ms?)` | Receive value or `nil` on timeout/close. |
| `nproc.channel_try_recv(ch)` | Non-blocking receive. |
| `nproc.channel_close(ch)` | Close and drop handle. |
| `nproc.queue()` | Unbounded queue alias. |
| `nproc.queue_put` / `queue_get` / `queue_try_get` / `queue_close` | Queue operations. |

## Shared memory

File-backed `MAP_SHARED` segments in the system temp directory (`niao_shm/`). Multiple processes (or handles) can `shared_open` the same name.

| Method | Description |
|--------|-------------|
| `nproc.shared_memory(name, size)` | Create segment (truncates existing). |
| `nproc.shared_open(name)` | Open existing segment. |
| `nproc.shared_read(shm, offset, len)` | Read bytes as string. |
| `nproc.shared_write(shm, offset, data)` | Write bytes/string; returns bytes written. |
| `nproc.shared_size(shm)` | Segment length. |
| `nproc.shared_unlink(name)` | Delete backing file. |

## Sync primitives (in-process)

| Method | Description |
|--------|-------------|
| `nproc.event()` | Auto-reset event. `event_set`, `event_clear`, `event_wait`, `event_is_set`. |
| `nproc.lock()` | Non-reentrant mutex. `lock_acquire`, `lock_try_acquire`, `lock_release`. |
| `nproc.semaphore(n)` | Counting semaphore. `semaphore_acquire`, `semaphore_try_acquire`, `semaphore_release`. |
| `nproc.barrier(parties)` | Thread barrier. `barrier_wait`. |

## Errors

| Code | Meaning |
|------|---------|
| 3500 | Wrong argument count. |
| 3501 | Spawn/I/O/timeout/closed resource (catchable `nproc_error`). |
| 3502 | Wrong argument type (hard error). |
| 3503 | Invalid or closed handle (catchable `nproc_error`). |

## Deferred / not in scope

- `fork` / copy-on-write process start (Niao VM is not fork-safe).
- `Pool.map` with Niao function callbacks (no cross-process closure serialization).
- `multiprocessing.Manager` proxy objects.
- Cross-process `Queue` / named semaphores (use pipes, spawn, or shared memory instead).

## See also

- `nshell` — one-shot subprocess run with captured output.
- `nos` — `getpid`, `system`, lightweight OS helpers.
- `parallel` — in-process thread pools and mutexes for sendable values.
- `nmmap` — read-only memory-mapped files.
