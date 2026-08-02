# nkeyring — OS credential stores

OS credential stores: macOS Keychain, Linux Secret Service, Windows Credential Manager (DPAPI-backed). ~Python [`keyring`](https://pypi.org/project/keyring/) subset.

## Import

```niao
import "nkeyring"
```

Paths `import "std/nkeyring"` and `import "nkeyring"` are equivalent. Flat builtins (`nkeyring_get_password`, `nkeyring_set_password`, …) are also available globally after import.

## Quick start

```niao
import "nkeyring"

// Store and retrieve (~keyring.set_password / get_password)
nkeyring.set_password("my-app", "api-token", "sk-live-abc123")
let token = nkeyring.get_password("my-app", "api-token")
print(token)

// Remove (~keyring.delete_password — errors if missing)
nkeyring.delete_password("my-app", "api-token")

// Reusable entry handle
let e = nkeyring.entry("my-app", "db-user")
nkeyring.set(e, "postgres://...")
print(nkeyring.get(e))
nkeyring.delete(e)
```

## Backends

| Platform | Store | `nkeyring.backend()` |
|----------|-------|----------------------|
| macOS | Keychain Services | `"keychain"` |
| Linux | Secret Service (GNOME Keyring / KWallet) | `"secret_service"` |
| Windows | Credential Manager (DPAPI) | `"windows_credential_manager"` |

Use `nkeyring.platform()` for `"macos"`, `"linux"`, or `"windows"`.

For unit tests and CI, switch to an in-memory backend (~Python `set_keyring` with a mock):

```niao
nkeyring.use_memory()
nkeyring.clear_memory()
// ... tests ...
nkeyring.use_system()
```

## Module-level API (~keyring)

| Method | Description |
|--------|-------------|
| `nkeyring.get_password(service, user)` | Read password string; `nil` if not found. |
| `nkeyring.set_password(service, user, password)` | Create or update password. |
| `nkeyring.delete_password(service, user)` | Delete credential; errors if missing. |
| `nkeyring.get_secret(service, user)` | Read binary secret as bytes; `nil` if missing. |
| `nkeyring.set_secret(service, user, secret)` | Store binary secret (string or bytes). |
| `nkeyring.get_credential(service, user)` | `{service, username, password}` or `nil`. |
| `nkeyring.exists(service, user)` | Whether a credential is stored. |

## Entry object API

| Method | Description |
|--------|-------------|
| `nkeyring.entry(service, user)` | Lightweight handle `{service, user, username, kind}`. |
| `nkeyring.get(entry)` | Password for entry; `nil` if missing. |
| `nkeyring.set(entry, password)` | Set password via entry handle. |
| `nkeyring.delete(entry)` | Delete via entry handle. |
| `nkeyring.get_bytes(entry)` | Binary secret; `nil` if missing. |
| `nkeyring.set_bytes(entry, secret)` | Store binary secret via entry. |

## Backend control

| Method | Description |
|--------|-------------|
| `nkeyring.backend()` | Active backend name (`"memory"` or OS store). |
| `nkeyring.platform()` | OS identifier string. |
| `nkeyring.use_memory()` | Route operations to in-memory store (this thread). |
| `nkeyring.use_system()` | Restore OS credential store (this thread). |
| `nkeyring.clear_memory()` | Wipe in-memory credentials. |

## Errors

| Code | Meaning |
|------|---------|
| `E3576` | Wrong argument count. |
| `E3577` | General `nkeyring_error` (invalid input, bad data, platform failure). |
| `E3578` | Type mismatch. |
| `E3579` | Credential not found (on `delete_password` / `delete`). |
| `E3580` | Store access denied or unavailable. |

`get_password`, `get_secret`, and `get_credential` return `nil` when a credential is missing (matching Python `get_password`).

## Notes

- **Service + user** together identify a credential (same convention as Python `keyring`).
- Passwords must be non-empty; empty secrets are rejected.
- The in-memory backend is thread-local; use `use_memory()` in each test thread.
- Linux requires a running Secret Service (e.g. GNOME Keyring). Headless CI should use `use_memory()`.
- Binary secrets use `set_secret` / `get_secret`; text passwords use `set_password` / `get_password`.

## Deferred

- Custom backend plugins (`set_keyring` with user-defined backends) — use `use_memory()` for tests instead.
- CLI `keyring get/set` subprocess wrapper — not needed; native store is built in.
- `keyring.backends` discovery listing all installed backends — only the platform default is exposed.

## Benchmark

```bash
cargo run -p niao_keyring --bin keyring_bench --release
niao run examples/nkeyring_bench.niao
```
