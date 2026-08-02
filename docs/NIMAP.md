# nimap — IMAP4 + POP3 mailbox retrieval

Native IMAP4rev1 and POP3 clients with TLS (rustls): search, flags, folders, FETCH, STORE, COPY/MOVE, EXPUNGE, and IDLE push. Parity targets Python `imaplib` / `imapclient` (IMAP subset) plus `poplib`.

## Import

```niao
import "nimap"
```

Paths `import "std/nimap"` and `import "nimap"` are equivalent. Flat builtins (`nimap_connect`, `nimap_search`, …) are also available globally after import.

## Quick start

```niao
import "nimap"

let c = nimap.connect({
    host: "imap.example.com",
    user: "alice@example.com",
    pass: "secret",
    tls: true,          // default; port 993
    mailbox: "INBOX",   // optional auto-SELECT
})

let ids = nimap.search(c, "UNSEEN")
let msgs = nimap.fetch(c, ids, "(FLAGS UID BODY.PEEK[])")
for m in msgs {
    let h = nimap.parse_headers(m.body)
    print(h.subject)
}

nimap.logout(c)
```

POP3:

```niao
let p = nimap.pop_connect({
    host: "pop.example.com",
    user: "alice",
    pass: "secret",
})
let st = nimap.pop_stat(p)
print(st.count, st.size)
let raw = nimap.pop_retr(p, 1)
nimap.pop_quit(p)
```

## Connect config

| Field | Type | Required | Default | Notes |
|-------|------|----------|---------|-------|
| `host` | string | yes | — | Server hostname. |
| `port` | int | no | 993/143 or 995/110 | Depends on `tls`. |
| `user` | string | yes | — | Login username. |
| `pass` | string | yes | — | Login password. |
| `tls` | bool | no | `true` | Implicit TLS (IMAPS/POP3S). |
| `starttls` | bool | no | `false` | STARTTLS / STLS after plain connect. |
| `timeout_ms` | int | no | `30000` | Socket timeout. |
| `mailbox` | string | no | — | IMAP only: SELECT after login. |

Returns a positive integer **handle**. Call `nimap.logout` / `nimap.close` (IMAP) or `nimap.pop_quit` (POP3) when done.

## IMAP — session

| Method | Description |
|--------|-------------|
| `nimap.connect(config)` | Connect, login, optional SELECT. |
| `nimap.logout(handle)` / `nimap.close(handle)` | LOGOUT and drop handle. |
| `nimap.capabilities(handle)` | Server capability strings. |
| `nimap.info(handle)` | `{protocol, host, port, mailbox?, …}`. |
| `nimap.noop(handle)` | NOOP (keep-alive / poll). |

## IMAP — folders

| Method | Description |
|--------|-------------|
| `nimap.list(handle, ref?, pattern?)` | LIST folders (default `""`, `"*"`). |
| `nimap.lsub(handle, ref?, pattern?)` | LSUB subscribed folders. |
| `nimap.select(handle, mailbox)` | SELECT (read-write). |
| `nimap.examine(handle, mailbox)` | EXAMINE (read-only). |
| `nimap.create` / `delete_mailbox` / `rename` | Mailbox management. |
| `nimap.subscribe` / `unsubscribe` | Subscription. |
| `nimap.status(handle, mailbox, items?)` | STATUS (MESSAGES, RECENT, …). |

## IMAP — search / fetch / flags

| Method | Description |
|--------|-------------|
| `nimap.search(handle, criteria, opts?)` | SEARCH; `opts.uid` for UID SEARCH. |
| `nimap.uid_search(handle, criteria)` | UID SEARCH. |
| `nimap.fetch(handle, set, items, opts?)` | FETCH; set is string or int array. |
| `nimap.uid_fetch(handle, set, items)` | UID FETCH. |
| `nimap.store(handle, set, flags, mode?, opts?)` | STORE flags; mode `"set"`/`"add"`/`"remove"`. |
| `nimap.uid_store(...)` | UID STORE. |
| `nimap.copy` / `uid_copy` | COPY messages. |
| `nimap.move(handle, set, mailbox, opts?)` | MOVE (or COPY+delete fallback). |
| `nimap.expunge(handle)` | EXPUNGE deleted messages. |
| `nimap.close_mailbox(handle)` | CLOSE selected mailbox. |

## IMAP — IDLE

| Method | Description |
|--------|-------------|
| `nimap.idle(handle, timeout_ms?)` | IDLE until event or timeout; returns `[{kind, value?}, …]`. |

## POP3

| Method | Description |
|--------|-------------|
| `nimap.pop_connect(config)` | Connect + USER/PASS. |
| `nimap.pop_stat` / `pop_list` / `pop_retr` / `pop_top` | Retrieve metadata and bodies. |
| `nimap.pop_dele` / `pop_uidl` / `pop_rset` / `pop_capa` | Delete, UIDL, reset, CAPA. |
| `nimap.pop_quit(handle)` | QUIT and drop handle. |

## Helpers (no network)

| Method | Description |
|--------|-------------|
| `nimap.parse_headers(raw)` | Lowercased header map from RFC822 text. |
| `nimap.quote(s)` | IMAP quoted string. |
| `nimap.message_set([1,2,3,9])` | Compact set `"1:3,9"`. |

## Errors

| Code | Meaning |
|------|---------|
| 4530 | Wrong argument count (thrown). |
| 4531 | I/O / TLS / general catchable `nimap_error`. |
| 4532 | Wrong argument type (thrown). |
| 4533 | Protocol / auth catchable `nimap_error`. |
| 4534 | Invalid or closed handle (catchable). |

## See also

- `nsmtp` — send mail.
- `nmail` — parse/build MIME messages.
- `nmime` — content-type sniffing.
