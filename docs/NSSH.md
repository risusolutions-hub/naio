# nssh — SSH client

Production SSH client for Niao: remote exec, interactive shell, SFTP, local port forwarding, password / key / agent auth. ~paramiko, fabric.

## Import

```niao
import "nssh"
```

Paths `import "std/nssh"` and `import "nssh"` are equivalent. Flat builtins (`nssh_connect`, `nssh_exec`, …) are also available globally after import.

## Quick start

```niao
import "nssh"

let s = nssh.connect({
    host: "example.com",
    user: "deploy",
    key: "/home/me/.ssh/id_ed25519",
    port: 22,
    timeout_ms: 10000
})

let r = nssh.exec(s, "uname -a")
print(r.stdout, "exit=", r.exit_status)

let sf = nssh.sftp_open(s)
nssh.sftp_write(sf, "/tmp/hello.txt", "from niao")
print(nssh.sftp_read(sf, "/tmp/hello.txt"))
nssh.sftp_close(sf)
nssh.close(s)
```

## Connect config

| Field | Type | Required | Default | Notes |
|-------|------|----------|---------|-------|
| `host` | string | yes | — | SSH hostname or IP. |
| `user` | string | yes | — | Remote username. |
| `port` | int | no | `22` | 0..=65535. |
| `password` | string | no | — | Password auth. |
| `key` / `key_path` | string | no | — | Path to private key. |
| `key_data` | string | no | — | PEM / OpenSSH private key text. |
| `passphrase` | string | no | — | Key passphrase. |
| `agent` | bool | no | `false` | Try local SSH agent (Pageant / OpenSSH pipe / `SSH_AUTH_SOCK`). |
| `timeout_ms` | int | no | 30000 | Connect / inactivity timeout. |

Auth tries password, then key, then agent (when enabled). First success wins. Missing required fields throw; connection/auth failures return catchable `nssh_error`.

## Session

| Method | Description |
|--------|-------------|
| `nssh.connect(config)` | Open a session; returns int handle. |
| `nssh.close(session)` | Disconnect. |
| `nssh.is_connected(session)` | `true` while the session is open. |
| `nssh.exec(session, command, opts?)` | Run a command. Returns `{stdout, stderr, stdout_bytes, stderr_bytes, exit_status, ok}`. `opts.timeout_ms` optional. |

## Interactive shell

| Method | Description |
|--------|-------------|
| `nssh.shell(session, opts?)` | Open a PTY shell. `opts`: `term`, `cols`, `rows`. |
| `nssh.shell_write(channel, data)` | Write string or bytes. |
| `nssh.shell_read(channel, opts?)` | Read string (or `nil` on EOF). `opts.timeout_ms`, `opts.max_bytes`. |
| `nssh.shell_close(channel)` | Close the channel. |

## SFTP

| Method | Description |
|--------|-------------|
| `nssh.sftp_open(session)` | Start the SFTP subsystem. |
| `nssh.sftp_close(sftp)` | Close SFTP. |
| `nssh.sftp_listdir(sftp, path)` | Entries: `{name, size, is_dir, is_file}`. |
| `nssh.sftp_stat(sftp, path)` | `{size, is_dir, is_file, permissions?}`. |
| `nssh.sftp_read(sftp, path)` | File bytes. |
| `nssh.sftp_write(sftp, path, data)` | Create/truncate write (string or bytes). |
| `nssh.sftp_mkdir` / `sftp_rmdir` / `sftp_remove` / `sftp_rename` | Path ops. |
| `nssh.sftp_get(sftp, remote, local)` | Download to local path. |
| `nssh.sftp_put(sftp, local, remote)` | Upload from local path. |

## Port forwarding

| Method | Description |
|--------|-------------|
| `nssh.forward_local(session, bind_port, remote_host, remote_port)` | Listen on `127.0.0.1:bind_port` (`0` = ephemeral). Returns `{id, bind_port, bind_addr}`. |
| `nssh.forward_close(forward)` | Stop the listener. |

## Keys & agent

| Method | Description |
|--------|-------------|
| `nssh.key_fingerprint(path_or_pem, opts?)` | SHA256 fingerprint. `opts.pem: true` treats input as key text; `opts.passphrase` optional. |
| `nssh.agent_identities()` | List agent identities (`fingerprint`, `algorithm`, `comment`), or catchable error if none. |

## Errors

| Code | Meaning |
|------|---------|
| 3600 | Wrong argument count. |
| 3601 | SSH / protocol / I/O error (catchable `nssh_error`). |
| 3602 | Wrong type or missing config field. |
| 3603 | Invalid or closed handle. |
| 3604 | Authentication failed. |

## Deferred (v0.1)

Remote port forwarding, ProxyJump, keyboard-interactive UI, SCP protocol (use SFTP), strict known_hosts policy APIs.

## See also

- `net` — lower-level TCP/TLS sockets.
- `nsmtp` — object-config network client style.
- `nws` — handle-based session API pattern.
