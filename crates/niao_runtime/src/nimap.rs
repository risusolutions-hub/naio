//! Native nimap standard library — IMAP4 + POP3 mailbox retrieval
//! (~imaplib, imapclient subset).
//!
//! Import with `import "nimap"` (or `import "std/nimap"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_imap::{
    format_message_set, imap_quote, parse_headers, ConnectOptions, FetchItem, Folder, IdleEvent,
    ImapClient, ImapError, MailboxStatus, PopClient, PopConnectOptions, PopListItem, PopStat,
    PopUidlItem, SelectData, StoreMode,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

const E4530: u32 = codes::E4530_NIMAP_ARITY;
const E4531: u32 = codes::E4531_NIMAP_ERROR;
const E4532: u32 = codes::E4532_NIMAP_TYPE;
const E4533: u32 = codes::E4533_NIMAP_PROTOCOL;
const E4534: u32 = codes::E4534_NIMAP_INVALID_HANDLE;

enum Session {
    Imap(ImapClient),
    Pop(PopClient),
}

thread_local! {
    static SESSIONS: RefCell<HashMap<i64, Session>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn new_id() -> i64 {
    NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4532, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4530,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4530,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn map_imap_err(span: Span, e: ImapError) -> ValueRef {
    let code = if e.is_protocol() { E4533 } else { E4531 };
    error_value(code, "nimap_error", e.message(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E4534,
        "nimap_error",
        format!("invalid or closed nimap handle {id}"),
        span,
    )
}

fn wrong_session(span: Span, expect: &str) -> ValueRef {
    error_value(
        E4531,
        "nimap_error",
        format!("handle is not an {expect} session"),
        span,
    )
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects positive handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Option<HashMap<String, ValueRef>>> {
    if args.len() <= idx {
        return Ok(None);
    }
    match &*args[idx].borrow() {
        Value::Nil => Ok(None),
        Value::Object(m) => Ok(Some(m.clone())),
        other => Err(type_err(
            span,
            format!("{name}() opts must be object, got {}", other.type_name()),
        )),
    }
}

fn config_arg(args: &[ValueRef], span: Span, name: &str) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[0].borrow() {
        Value::Object(map) => Ok(map.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a config object, got {}",
                other.type_name()
            ),
        )),
    }
}

fn required_string(
    config: &HashMap<String, ValueRef>,
    field: &str,
    span: Span,
) -> NiaoResult<String> {
    match config.get(field) {
        Some(v) => match &*v.borrow() {
            Value::String(s) if !s.is_empty() => Ok(s.clone()),
            Value::String(_) => Err(type_err(span, format!("config.{field} must not be empty"))),
            other => Err(type_err(
                span,
                format!(
                    "config.{field} must be a string, got {}",
                    other.type_name()
                ),
            )),
        },
        None => Err(type_err(span, format!("config: missing field '{field}'"))),
    }
}

fn optional_string(config: &HashMap<String, ValueRef>, field: &str) -> Option<String> {
    config.get(field).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn optional_bool(config: &HashMap<String, ValueRef>, field: &str, default: bool) -> bool {
    match config.get(field) {
        Some(v) => match &*v.borrow() {
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::String(s) => matches!(s.as_str(), "true" | "1" | "yes" | "on"),
            _ => default,
        },
        None => default,
    }
}

fn optional_port(
    config: &HashMap<String, ValueRef>,
    span: Span,
    default: u16,
) -> NiaoResult<u16> {
    match config.get("port") {
        None => Ok(default),
        Some(v) => match &*v.borrow() {
            Value::Nil => Ok(default),
            Value::Int(n) if (0..=65535).contains(n) => Ok(*n as u16),
            Value::Int(_) => Err(type_err(span, "config.port must be 0..=65535")),
            other => Err(type_err(
                span,
                format!("config.port must be an int, got {}", other.type_name()),
            )),
        },
    }
}

fn optional_timeout_ms(config: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<u64> {
    match config.get("timeout_ms") {
        None => Ok(30_000),
        Some(v) => match &*v.borrow() {
            Value::Nil => Ok(30_000),
            Value::Int(n) if *n > 0 => Ok(*n as u64),
            Value::Int(_) => Err(type_err(span, "config.timeout_ms must be positive")),
            other => Err(type_err(
                span,
                format!(
                    "config.timeout_ms must be an int, got {}",
                    other.type_name()
                ),
            )),
        },
    }
}

fn parse_imap_connect(
    config: &HashMap<String, ValueRef>,
    span: Span,
) -> NiaoResult<ConnectOptions> {
    let host = required_string(config, "host", span)?;
    let tls = optional_bool(config, "tls", true);
    let port = optional_port(config, span, ConnectOptions::default_port(tls))?;
    let user = optional_string(config, "user").unwrap_or_default();
    let pass = optional_string(config, "pass").unwrap_or_default();
    let starttls = optional_bool(config, "starttls", false);
    let timeout_ms = optional_timeout_ms(config, span)?;
    let mailbox = optional_string(config, "mailbox").filter(|s| !s.is_empty());
    Ok(ConnectOptions {
        host,
        port,
        user,
        pass,
        tls,
        starttls,
        timeout: Duration::from_millis(timeout_ms),
        mailbox,
    })
}

fn parse_pop_connect(
    config: &HashMap<String, ValueRef>,
    span: Span,
) -> NiaoResult<PopConnectOptions> {
    let host = required_string(config, "host", span)?;
    let tls = optional_bool(config, "tls", true);
    let port = optional_port(config, span, PopConnectOptions::default_port(tls))?;
    let user = optional_string(config, "user").unwrap_or_default();
    let pass = optional_string(config, "pass").unwrap_or_default();
    let starttls = optional_bool(config, "starttls", false);
    let timeout_ms = optional_timeout_ms(config, span)?;
    Ok(PopConnectOptions {
        host,
        port,
        user,
        pass,
        tls,
        starttls,
        timeout: Duration::from_millis(timeout_ms),
    })
}

fn with_imap<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&mut ImapClient) -> NiaoResult<ValueRef>,
{
    SESSIONS.with(|s| {
        let mut map = s.borrow_mut();
        match map.get_mut(&id) {
            Some(Session::Imap(c)) => f(c),
            Some(Session::Pop(_)) => Ok(wrong_session(span, "IMAP")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn with_pop<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&mut PopClient) -> NiaoResult<ValueRef>,
{
    SESSIONS.with(|s| {
        let mut map = s.borrow_mut();
        match map.get_mut(&id) {
            Some(Session::Pop(c)) => f(c),
            Some(Session::Imap(_)) => Ok(wrong_session(span, "POP3")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn folder_to_value(f: &Folder) -> Value {
    let mut map = HashMap::new();
    map.insert("name".into(), Value::String(f.name.clone()).ref_cell());
    map.insert(
        "delimiter".into(),
        Value::String(f.delimiter.clone()).ref_cell(),
    );
    map.insert(
        "attrs".into(),
        Value::Array(
            f.attrs
                .iter()
                .map(|a| Value::String(a.clone()).ref_cell())
                .collect(),
        )
        .ref_cell(),
    );
    Value::Object(map)
}

fn select_to_value(d: &SelectData) -> Value {
    let mut map = HashMap::new();
    map.insert("mailbox".into(), Value::String(d.mailbox.clone()).ref_cell());
    map.insert("exists".into(), Value::Int(d.exists as i64).ref_cell());
    map.insert("recent".into(), Value::Int(d.recent as i64).ref_cell());
    map.insert(
        "uidnext".into(),
        d.uidnext
            .map(|n| Value::Int(n as i64).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    map.insert(
        "uidvalidity".into(),
        d.uidvalidity
            .map(|n| Value::Int(n as i64).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    map.insert(
        "unseen".into(),
        d.unseen
            .map(|n| Value::Int(n as i64).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    map.insert(
        "flags".into(),
        Value::Array(
            d.flags
                .iter()
                .map(|f| Value::String(f.clone()).ref_cell())
                .collect(),
        )
        .ref_cell(),
    );
    map.insert(
        "permanent_flags".into(),
        Value::Array(
            d.permanent_flags
                .iter()
                .map(|f| Value::String(f.clone()).ref_cell())
                .collect(),
        )
        .ref_cell(),
    );
    map.insert("readonly".into(), Value::Bool(d.readonly).ref_cell());
    Value::Object(map)
}

fn status_to_value(s: &MailboxStatus) -> Value {
    let mut map = HashMap::new();
    map.insert("mailbox".into(), Value::String(s.mailbox.clone()).ref_cell());
    if let Some(n) = s.messages {
        map.insert("messages".into(), Value::Int(n as i64).ref_cell());
    }
    if let Some(n) = s.recent {
        map.insert("recent".into(), Value::Int(n as i64).ref_cell());
    }
    if let Some(n) = s.uidnext {
        map.insert("uidnext".into(), Value::Int(n as i64).ref_cell());
    }
    if let Some(n) = s.uidvalidity {
        map.insert("uidvalidity".into(), Value::Int(n as i64).ref_cell());
    }
    if let Some(n) = s.unseen {
        map.insert("unseen".into(), Value::Int(n as i64).ref_cell());
    }
    Value::Object(map)
}

fn fetch_item_to_value(item: &FetchItem) -> Value {
    let mut map = HashMap::new();
    map.insert("seq".into(), Value::Int(item.seq as i64).ref_cell());
    map.insert(
        "uid".into(),
        item.uid
            .map(|n| Value::Int(n as i64).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    map.insert(
        "flags".into(),
        Value::Array(
            item.flags
                .iter()
                .map(|f| Value::String(f.clone()).ref_cell())
                .collect(),
        )
        .ref_cell(),
    );
    map.insert(
        "size".into(),
        item.size
            .map(|n| Value::Int(n as i64).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    map.insert(
        "body".into(),
        item.body
            .as_ref()
            .map(|s| Value::String(s.clone()).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    map.insert("raw".into(), Value::String(item.raw.clone()).ref_cell());
    Value::Object(map)
}

fn idle_event_to_value(ev: &IdleEvent) -> Value {
    let mut map = HashMap::new();
    map.insert(
        "kind".into(),
        Value::String(ev.kind_name().to_string()).ref_cell(),
    );
    match ev {
        IdleEvent::Other(s) => {
            map.insert("value".into(), Value::String(s.clone()).ref_cell());
        }
        _ => {
            if let Some(n) = ev.value() {
                map.insert("value".into(), Value::Int(n as i64).ref_cell());
            }
        }
    }
    Value::Object(map)
}

fn pop_stat_to_value(s: &PopStat) -> Value {
    let mut map = HashMap::new();
    map.insert("count".into(), Value::Int(s.count as i64).ref_cell());
    map.insert("size".into(), Value::Int(s.size as i64).ref_cell());
    Value::Object(map)
}

fn pop_list_item_to_value(item: &PopListItem) -> Value {
    let mut map = HashMap::new();
    map.insert("msg".into(), Value::Int(item.msg as i64).ref_cell());
    map.insert("size".into(), Value::Int(item.size as i64).ref_cell());
    Value::Object(map)
}

fn pop_uidl_item_to_value(item: &PopUidlItem) -> Value {
    let mut map = HashMap::new();
    map.insert("msg".into(), Value::Int(item.msg as i64).ref_cell());
    map.insert("uid".into(), Value::String(item.uid.clone()).ref_cell());
    Value::Object(map)
}

fn message_set_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        Value::Array(items) => {
            let mut ids = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Int(n) if *n > 0 => ids.push(*n as u32),
                    Value::Int(_) => {
                        return Err(type_err(
                            span,
                            format!("{name}() message id must be positive int (item {})", i + 1),
                        ));
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() message set array items must be ints, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(format_message_set(&ids))
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string or int[] message set, got {}",
                other.type_name()
            ),
        )),
    }
}

fn flags_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<String>> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(vec![s.clone()]),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() flags array item {} must be string, got {}",
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string or string[] flags, got {}",
                other.type_name()
            ),
        )),
    }
}

fn store_mode_from_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<StoreMode> {
    if args.len() <= idx {
        return Ok(StoreMode::Set);
    }
    match &*args[idx].borrow() {
        Value::Nil => Ok(StoreMode::Set),
        Value::String(s) => Ok(StoreMode::parse(s)),
        other => Err(type_err(
            span,
            format!(
                "{name}() mode must be string, got {}",
                other.type_name()
            ),
        )),
    }
}

fn opts_uid(opts: Option<&HashMap<String, ValueRef>>, default: bool) -> bool {
    let Some(map) = opts else {
        return default;
    };
    match map.get("uid").map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        _ => default,
    }
}

fn optional_string_list(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Vec<String>> {
    if args.len() <= idx {
        return Ok(Vec::new());
    }
    match &*args[idx].borrow() {
        Value::Nil => Ok(Vec::new()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() items array item {} must be string, got {}",
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        Value::String(s) => Ok(s.split_whitespace().map(str::to_string).collect()),
        other => Err(type_err(
            span,
            format!(
                "{name}() items must be string or string[], got {}",
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins — IMAP
// ---------------------------------------------------------------------------

// >>> type(nimap.connect) == "native"
fn nimap_connect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_connect", span)?;
    let config = config_arg(args, span, "nimap_connect")?;
    let opts = parse_imap_connect(&config, span)?;
    match ImapClient::connect(&opts) {
        Ok(client) => {
            let id = new_id();
            SESSIONS.with(|s| s.borrow_mut().insert(id, Session::Imap(client)));
            Ok(Value::Int(id).ref_cell())
        }
        Err(e) => Ok(map_imap_err(span, e)),
    }
}

// >>> type(nimap.logout) == "native"
fn nimap_logout(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_logout", span)?;
    let id = handle_arg(args, 0, "nimap_logout", span)?;
    with_imap(id, span, |c| match c.logout() {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.close) == "native"
fn nimap_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_close", span)?;
    let id = handle_arg(args, 0, "nimap_close", span)?;
    SESSIONS.with(|s| {
        let mut map = s.borrow_mut();
        match map.get_mut(&id) {
            Some(Session::Imap(c)) => {
                let result = match c.logout() {
                    Ok(()) => Value::Bool(true).ref_cell(),
                    Err(e) => map_imap_err(span, e),
                };
                map.remove(&id);
                Ok(result)
            }
            Some(Session::Pop(_)) => Ok(wrong_session(span, "IMAP")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> type(nimap.capabilities) == "native"
fn nimap_capabilities(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_capabilities", span)?;
    let id = handle_arg(args, 0, "nimap_capabilities", span)?;
    with_imap(id, span, |c| {
        match c.refresh_capabilities() {
            Ok(caps) => Ok(Value::Array(
                caps.iter()
                    .map(|s| Value::String(s.clone()).ref_cell())
                    .collect(),
            )
            .ref_cell()),
            Err(e) => Ok(map_imap_err(span, e)),
        }
    })
}

// >>> type(nimap.info) == "native"
fn nimap_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_info", span)?;
    let id = handle_arg(args, 0, "nimap_info", span)?;
    SESSIONS.with(|s| {
        let map = s.borrow();
        match map.get(&id) {
            Some(Session::Imap(c)) => {
                let mut out = HashMap::new();
                out.insert("protocol".into(), Value::String("imap".into()).ref_cell());
                out.insert("host".into(), Value::String(c.host.clone()).ref_cell());
                out.insert("port".into(), Value::Int(c.port as i64).ref_cell());
                out.insert(
                    "capabilities".into(),
                    Value::Array(
                        c.capabilities
                            .iter()
                            .map(|x| Value::String(x.clone()).ref_cell())
                            .collect(),
                    )
                    .ref_cell(),
                );
                out.insert(
                    "selected".into(),
                    c.selected
                        .as_ref()
                        .map(|d| select_to_value(d).ref_cell())
                        .unwrap_or_else(|| Value::Nil.ref_cell()),
                );
                Ok(Value::Object(out).ref_cell())
            }
            Some(Session::Pop(c)) => {
                let mut out = HashMap::new();
                out.insert("protocol".into(), Value::String("pop3".into()).ref_cell());
                out.insert("host".into(), Value::String(c.host.clone()).ref_cell());
                out.insert("port".into(), Value::Int(c.port as i64).ref_cell());
                Ok(Value::Object(out).ref_cell())
            }
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> type(nimap.noop) == "native"
fn nimap_noop(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_noop", span)?;
    let id = handle_arg(args, 0, "nimap_noop", span)?;
    with_imap(id, span, |c| match c.noop() {
        Ok(lines) => Ok(Value::Array(
            lines
                .into_iter()
                .map(|l| Value::String(l).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.list) == "native"
fn nimap_list(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nimap_list", span)?;
    let id = handle_arg(args, 0, "nimap_list", span)?;
    let reference = if args.len() > 1 {
        match &*args[1].borrow() {
            Value::Nil => "".to_string(),
            Value::String(s) => s.clone(),
            other => {
                return Err(type_err(
                    span,
                    format!("ref must be string, got {}", other.type_name()),
                ));
            }
        }
    } else {
        String::new()
    };
    let pattern = if args.len() > 2 {
        match &*args[2].borrow() {
            Value::Nil => "*".to_string(),
            Value::String(s) => s.clone(),
            other => {
                return Err(type_err(
                    span,
                    format!("pattern must be string, got {}", other.type_name()),
                ));
            }
        }
    } else {
        "*".to_string()
    };
    with_imap(id, span, |c| match c.list(&reference, &pattern) {
        Ok(folders) => Ok(Value::Array(
            folders
                .iter()
                .map(|f| folder_to_value(f).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.lsub) == "native"
fn nimap_lsub(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nimap_lsub", span)?;
    let id = handle_arg(args, 0, "nimap_lsub", span)?;
    let reference = if args.len() > 1 {
        match &*args[1].borrow() {
            Value::Nil => "".to_string(),
            Value::String(s) => s.clone(),
            other => {
                return Err(type_err(
                    span,
                    format!("ref must be string, got {}", other.type_name()),
                ));
            }
        }
    } else {
        String::new()
    };
    let pattern = if args.len() > 2 {
        match &*args[2].borrow() {
            Value::Nil => "*".to_string(),
            Value::String(s) => s.clone(),
            other => {
                return Err(type_err(
                    span,
                    format!("pattern must be string, got {}", other.type_name()),
                ));
            }
        }
    } else {
        "*".to_string()
    };
    with_imap(id, span, |c| match c.lsub(&reference, &pattern) {
        Ok(folders) => Ok(Value::Array(
            folders
                .iter()
                .map(|f| folder_to_value(f).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.select) == "native"
fn nimap_select(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nimap_select", span)?;
    let id = handle_arg(args, 0, "nimap_select", span)?;
    let mailbox = string_arg(args, 1, "nimap_select", span)?;
    with_imap(id, span, |c| match c.select(&mailbox) {
        Ok(data) => Ok(select_to_value(&data).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.examine) == "native"
fn nimap_examine(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nimap_examine", span)?;
    let id = handle_arg(args, 0, "nimap_examine", span)?;
    let mailbox = string_arg(args, 1, "nimap_examine", span)?;
    with_imap(id, span, |c| match c.examine(&mailbox) {
        Ok(data) => Ok(select_to_value(&data).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.create) == "native"
fn nimap_create(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nimap_create", span)?;
    let id = handle_arg(args, 0, "nimap_create", span)?;
    let mailbox = string_arg(args, 1, "nimap_create", span)?;
    with_imap(id, span, |c| match c.create(&mailbox) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.delete_mailbox) == "native"
fn nimap_delete_mailbox(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nimap_delete_mailbox", span)?;
    let id = handle_arg(args, 0, "nimap_delete_mailbox", span)?;
    let mailbox = string_arg(args, 1, "nimap_delete_mailbox", span)?;
    with_imap(id, span, |c| match c.delete_mailbox(&mailbox) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.rename) == "native"
fn nimap_rename(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nimap_rename", span)?;
    let id = handle_arg(args, 0, "nimap_rename", span)?;
    let old = string_arg(args, 1, "nimap_rename", span)?;
    let new = string_arg(args, 2, "nimap_rename", span)?;
    with_imap(id, span, |c| match c.rename(&old, &new) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.subscribe) == "native"
fn nimap_subscribe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nimap_subscribe", span)?;
    let id = handle_arg(args, 0, "nimap_subscribe", span)?;
    let mailbox = string_arg(args, 1, "nimap_subscribe", span)?;
    with_imap(id, span, |c| match c.subscribe(&mailbox) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.unsubscribe) == "native"
fn nimap_unsubscribe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nimap_unsubscribe", span)?;
    let id = handle_arg(args, 0, "nimap_unsubscribe", span)?;
    let mailbox = string_arg(args, 1, "nimap_unsubscribe", span)?;
    with_imap(id, span, |c| match c.unsubscribe(&mailbox) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.status) == "native"
fn nimap_status(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nimap_status", span)?;
    let id = handle_arg(args, 0, "nimap_status", span)?;
    let mailbox = string_arg(args, 1, "nimap_status", span)?;
    let items = optional_string_list(args, 2, "nimap_status", span)?;
    let item_refs: Vec<&str> = items.iter().map(String::as_str).collect();
    with_imap(id, span, |c| match c.status(&mailbox, &item_refs) {
        Ok(st) => Ok(status_to_value(&st).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.search) == "native"
fn nimap_search(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nimap_search", span)?;
    let id = handle_arg(args, 0, "nimap_search", span)?;
    let criteria = string_arg(args, 1, "nimap_search", span)?;
    let opts = optional_object(args, 2, "nimap_search", span)?;
    let uid = opts_uid(opts.as_ref(), false);
    with_imap(id, span, |c| match c.search(&criteria, uid) {
        Ok(ids) => Ok(Value::Array(
            ids.into_iter()
                .map(|n| Value::Int(n as i64).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.uid_search) == "native"
fn nimap_uid_search(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nimap_uid_search", span)?;
    let id = handle_arg(args, 0, "nimap_uid_search", span)?;
    let criteria = string_arg(args, 1, "nimap_uid_search", span)?;
    with_imap(id, span, |c| match c.search(&criteria, true) {
        Ok(ids) => Ok(Value::Array(
            ids.into_iter()
                .map(|n| Value::Int(n as i64).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.fetch) == "native"
fn nimap_fetch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nimap_fetch", span)?;
    let id = handle_arg(args, 0, "nimap_fetch", span)?;
    let set = message_set_arg(args, 1, "nimap_fetch", span)?;
    let items = string_arg(args, 2, "nimap_fetch", span)?;
    let opts = optional_object(args, 3, "nimap_fetch", span)?;
    let uid = opts_uid(opts.as_ref(), false);
    with_imap(id, span, |c| match c.fetch(&set, &items, uid) {
        Ok(fetched) => Ok(Value::Array(
            fetched
                .iter()
                .map(|f| fetch_item_to_value(f).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.uid_fetch) == "native"
fn nimap_uid_fetch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nimap_uid_fetch", span)?;
    let id = handle_arg(args, 0, "nimap_uid_fetch", span)?;
    let set = message_set_arg(args, 1, "nimap_uid_fetch", span)?;
    let items = string_arg(args, 2, "nimap_uid_fetch", span)?;
    with_imap(id, span, |c| match c.fetch(&set, &items, true) {
        Ok(fetched) => Ok(Value::Array(
            fetched
                .iter()
                .map(|f| fetch_item_to_value(f).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.store) == "native"
fn nimap_store(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 5, "nimap_store", span)?;
    let id = handle_arg(args, 0, "nimap_store", span)?;
    let set = message_set_arg(args, 1, "nimap_store", span)?;
    let flags = flags_arg(args, 2, "nimap_store", span)?;
    // mode? then opts?; a lone 4th object is treated as opts (mode=Set).
    let (mode, uid) = if args.len() >= 5 {
        let mode = store_mode_from_arg(args, 3, "nimap_store", span)?;
        let opts = optional_object(args, 4, "nimap_store", span)?;
        (mode, opts_uid(opts.as_ref(), false))
    } else if args.len() == 4 {
        match &*args[3].borrow() {
            Value::Object(m) => (StoreMode::Set, opts_uid(Some(m), false)),
            Value::Nil => (StoreMode::Set, false),
            Value::String(s) => (StoreMode::parse(s), false),
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "nimap_store() mode must be string or opts object, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    } else {
        (StoreMode::Set, false)
    };
    with_imap(id, span, |c| match c.store(&set, &flags, mode, uid) {
        Ok(fetched) => Ok(Value::Array(
            fetched
                .iter()
                .map(|f| fetch_item_to_value(f).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.uid_store) == "native"
fn nimap_uid_store(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nimap_uid_store", span)?;
    let id = handle_arg(args, 0, "nimap_uid_store", span)?;
    let set = message_set_arg(args, 1, "nimap_uid_store", span)?;
    let flags = flags_arg(args, 2, "nimap_uid_store", span)?;
    let mode = store_mode_from_arg(args, 3, "nimap_uid_store", span)?;
    with_imap(id, span, |c| match c.store(&set, &flags, mode, true) {
        Ok(fetched) => Ok(Value::Array(
            fetched
                .iter()
                .map(|f| fetch_item_to_value(f).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.copy) == "native"
fn nimap_copy(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nimap_copy", span)?;
    let id = handle_arg(args, 0, "nimap_copy", span)?;
    let set = message_set_arg(args, 1, "nimap_copy", span)?;
    let mailbox = string_arg(args, 2, "nimap_copy", span)?;
    let opts = optional_object(args, 3, "nimap_copy", span)?;
    let uid = opts_uid(opts.as_ref(), false);
    with_imap(id, span, |c| match c.copy(&set, &mailbox, uid) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.uid_copy) == "native"
fn nimap_uid_copy(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nimap_uid_copy", span)?;
    let id = handle_arg(args, 0, "nimap_uid_copy", span)?;
    let set = message_set_arg(args, 1, "nimap_uid_copy", span)?;
    let mailbox = string_arg(args, 2, "nimap_uid_copy", span)?;
    with_imap(id, span, |c| match c.copy(&set, &mailbox, true) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.move) == "native"
fn nimap_move(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nimap_move", span)?;
    let id = handle_arg(args, 0, "nimap_move", span)?;
    let set = message_set_arg(args, 1, "nimap_move", span)?;
    let mailbox = string_arg(args, 2, "nimap_move", span)?;
    let opts = optional_object(args, 3, "nimap_move", span)?;
    let uid = opts_uid(opts.as_ref(), false);
    with_imap(id, span, |c| match c.move_msgs(&set, &mailbox, uid) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.expunge) == "native"
fn nimap_expunge(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_expunge", span)?;
    let id = handle_arg(args, 0, "nimap_expunge", span)?;
    with_imap(id, span, |c| match c.expunge() {
        Ok(seqs) => Ok(Value::Array(
            seqs.into_iter()
                .map(|n| Value::Int(n as i64).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.close_mailbox) == "native"
fn nimap_close_mailbox(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_close_mailbox", span)?;
    let id = handle_arg(args, 0, "nimap_close_mailbox", span)?;
    with_imap(id, span, |c| match c.close_mailbox() {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.idle) == "native"
fn nimap_idle(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nimap_idle", span)?;
    let id = handle_arg(args, 0, "nimap_idle", span)?;
    let timeout_ms = if args.len() > 1 {
        match &*args[1].borrow() {
            Value::Nil => 30_000i64,
            Value::Int(n) if *n > 0 => *n,
            Value::Int(_) => {
                return Err(type_err(span, "timeout_ms must be positive"));
            }
            other => {
                return Err(type_err(
                    span,
                    format!("timeout_ms must be int, got {}", other.type_name()),
                ));
            }
        }
    } else {
        30_000
    };
    with_imap(id, span, |c| {
        match c.idle(Duration::from_millis(timeout_ms as u64)) {
            Ok(events) => Ok(Value::Array(
                events
                    .iter()
                    .map(|e| idle_event_to_value(e).ref_cell())
                    .collect(),
            )
            .ref_cell()),
            Err(e) => Ok(map_imap_err(span, e)),
        }
    })
}

// ---------------------------------------------------------------------------
// Builtins — POP3
// ---------------------------------------------------------------------------

// >>> type(nimap.pop_connect) == "native"
fn nimap_pop_connect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_pop_connect", span)?;
    let config = config_arg(args, span, "nimap_pop_connect")?;
    let opts = parse_pop_connect(&config, span)?;
    match PopClient::connect(&opts) {
        Ok(client) => {
            let id = new_id();
            SESSIONS.with(|s| s.borrow_mut().insert(id, Session::Pop(client)));
            Ok(Value::Int(id).ref_cell())
        }
        Err(e) => Ok(map_imap_err(span, e)),
    }
}

// >>> type(nimap.pop_stat) == "native"
fn nimap_pop_stat(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_pop_stat", span)?;
    let id = handle_arg(args, 0, "nimap_pop_stat", span)?;
    with_pop(id, span, |c| match c.stat() {
        Ok(st) => Ok(pop_stat_to_value(&st).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.pop_list) == "native"
fn nimap_pop_list(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nimap_pop_list", span)?;
    let id = handle_arg(args, 0, "nimap_pop_list", span)?;
    let msg = if args.len() > 1 {
        match &*args[1].borrow() {
            Value::Nil => None,
            Value::Int(n) if *n > 0 => Some(*n as u32),
            Value::Int(_) => {
                return Err(type_err(span, "msg must be positive int"));
            }
            other => {
                return Err(type_err(
                    span,
                    format!("msg must be int, got {}", other.type_name()),
                ));
            }
        }
    } else {
        None
    };
    with_pop(id, span, |c| match c.list(msg) {
        Ok(items) => Ok(Value::Array(
            items
                .iter()
                .map(|i| pop_list_item_to_value(i).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.pop_retr) == "native"
fn nimap_pop_retr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nimap_pop_retr", span)?;
    let id = handle_arg(args, 0, "nimap_pop_retr", span)?;
    let msg = int_arg(args, 1, "nimap_pop_retr", span)?;
    if msg <= 0 {
        return Err(type_err(span, "msg must be positive"));
    }
    with_pop(id, span, |c| match c.retr(msg as u32) {
        Ok(raw) => Ok(Value::String(raw).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.pop_top) == "native"
fn nimap_pop_top(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nimap_pop_top", span)?;
    let id = handle_arg(args, 0, "nimap_pop_top", span)?;
    let msg = int_arg(args, 1, "nimap_pop_top", span)?;
    let lines = int_arg(args, 2, "nimap_pop_top", span)?;
    if msg <= 0 {
        return Err(type_err(span, "msg must be positive"));
    }
    if lines < 0 {
        return Err(type_err(span, "lines must be non-negative"));
    }
    with_pop(id, span, |c| match c.top(msg as u32, lines as u32) {
        Ok(raw) => Ok(Value::String(raw).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.pop_dele) == "native"
fn nimap_pop_dele(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nimap_pop_dele", span)?;
    let id = handle_arg(args, 0, "nimap_pop_dele", span)?;
    let msg = int_arg(args, 1, "nimap_pop_dele", span)?;
    if msg <= 0 {
        return Err(type_err(span, "msg must be positive"));
    }
    with_pop(id, span, |c| match c.dele(msg as u32) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.pop_uidl) == "native"
fn nimap_pop_uidl(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nimap_pop_uidl", span)?;
    let id = handle_arg(args, 0, "nimap_pop_uidl", span)?;
    let msg = if args.len() > 1 {
        match &*args[1].borrow() {
            Value::Nil => None,
            Value::Int(n) if *n > 0 => Some(*n as u32),
            Value::Int(_) => {
                return Err(type_err(span, "msg must be positive int"));
            }
            other => {
                return Err(type_err(
                    span,
                    format!("msg must be int, got {}", other.type_name()),
                ));
            }
        }
    } else {
        None
    };
    with_pop(id, span, |c| match c.uidl(msg) {
        Ok(items) => Ok(Value::Array(
            items
                .iter()
                .map(|i| pop_uidl_item_to_value(i).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.pop_rset) == "native"
fn nimap_pop_rset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_pop_rset", span)?;
    let id = handle_arg(args, 0, "nimap_pop_rset", span)?;
    with_pop(id, span, |c| match c.rset() {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.pop_capa) == "native"
fn nimap_pop_capa(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_pop_capa", span)?;
    let id = handle_arg(args, 0, "nimap_pop_capa", span)?;
    with_pop(id, span, |c| match c.capa() {
        Ok(caps) => Ok(Value::Array(
            caps.into_iter()
                .map(|s| Value::String(s).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(map_imap_err(span, e)),
    })
}

// >>> type(nimap.pop_quit) == "native"
fn nimap_pop_quit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_pop_quit", span)?;
    let id = handle_arg(args, 0, "nimap_pop_quit", span)?;
    SESSIONS.with(|s| {
        let mut map = s.borrow_mut();
        match map.get_mut(&id) {
            Some(Session::Pop(c)) => {
                let result = match c.quit() {
                    Ok(()) => Value::Bool(true).ref_cell(),
                    Err(e) => map_imap_err(span, e),
                };
                map.remove(&id);
                Ok(result)
            }
            Some(Session::Imap(_)) => Ok(wrong_session(span, "POP3")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// ---------------------------------------------------------------------------
// Helpers (no handle)
// ---------------------------------------------------------------------------

// >>> nimap.parse_headers("Subject: Hi\r\n\r\n")["subject"]
// => "Hi"
fn nimap_parse_headers(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_parse_headers", span)?;
    let raw = string_arg(args, 0, "nimap_parse_headers", span)?;
    let headers = parse_headers(&raw);
    let mut map = HashMap::new();
    for (k, v) in headers {
        map.insert(k, Value::String(v).ref_cell());
    }
    Ok(Value::Object(map).ref_cell())
}

// >>> nimap.quote("a")
// => "\"a\""
fn nimap_quote(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_quote", span)?;
    let s = string_arg(args, 0, "nimap_quote", span)?;
    Ok(Value::String(imap_quote(&s)).ref_cell())
}

// >>> nimap.message_set([1, 2, 3, 9])
// => "1:3,9"
fn nimap_message_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nimap_message_set", span)?;
    let set = message_set_arg(args, 0, "nimap_message_set", span)?;
    Ok(Value::String(set).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nimap_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nimap_fns![
    ("nimap_connect", "connect", nimap_connect),
    ("nimap_logout", "logout", nimap_logout),
    ("nimap_close", "close", nimap_close),
    ("nimap_capabilities", "capabilities", nimap_capabilities),
    ("nimap_info", "info", nimap_info),
    ("nimap_noop", "noop", nimap_noop),
    ("nimap_list", "list", nimap_list),
    ("nimap_lsub", "lsub", nimap_lsub),
    ("nimap_select", "select", nimap_select),
    ("nimap_examine", "examine", nimap_examine),
    ("nimap_create", "create", nimap_create),
    ("nimap_delete_mailbox", "delete_mailbox", nimap_delete_mailbox),
    ("nimap_rename", "rename", nimap_rename),
    ("nimap_subscribe", "subscribe", nimap_subscribe),
    ("nimap_unsubscribe", "unsubscribe", nimap_unsubscribe),
    ("nimap_status", "status", nimap_status),
    ("nimap_search", "search", nimap_search),
    ("nimap_uid_search", "uid_search", nimap_uid_search),
    ("nimap_fetch", "fetch", nimap_fetch),
    ("nimap_uid_fetch", "uid_fetch", nimap_uid_fetch),
    ("nimap_store", "store", nimap_store),
    ("nimap_uid_store", "uid_store", nimap_uid_store),
    ("nimap_copy", "copy", nimap_copy),
    ("nimap_uid_copy", "uid_copy", nimap_uid_copy),
    ("nimap_move", "move", nimap_move),
    ("nimap_expunge", "expunge", nimap_expunge),
    ("nimap_close_mailbox", "close_mailbox", nimap_close_mailbox),
    ("nimap_idle", "idle", nimap_idle),
    ("nimap_pop_connect", "pop_connect", nimap_pop_connect),
    ("nimap_pop_stat", "pop_stat", nimap_pop_stat),
    ("nimap_pop_list", "pop_list", nimap_pop_list),
    ("nimap_pop_retr", "pop_retr", nimap_pop_retr),
    ("nimap_pop_top", "pop_top", nimap_pop_top),
    ("nimap_pop_dele", "pop_dele", nimap_pop_dele),
    ("nimap_pop_uidl", "pop_uidl", nimap_pop_uidl),
    ("nimap_pop_rset", "pop_rset", nimap_pop_rset),
    ("nimap_pop_capa", "pop_capa", nimap_pop_capa),
    ("nimap_pop_quit", "pop_quit", nimap_pop_quit),
    ("nimap_parse_headers", "parse_headers", nimap_parse_headers),
    ("nimap_quote", "quote", nimap_quote),
    ("nimap_message_set", "message_set", nimap_message_set),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nimap";
pub const MODULE_PATHS: &[&str] = &["nimap", "std/nimap"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values_equal;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn quote_doctest() {
        let v = nimap_quote(&[Value::String("a".into()).ref_cell()], span()).unwrap();
        let got = v.borrow().clone();
        assert!(values_equal(&got, &Value::String("\"a\"".into())));
    }

    #[test]
    fn message_set_doctest() {
        let ids = Value::Array(vec![
            Value::Int(1).ref_cell(),
            Value::Int(2).ref_cell(),
            Value::Int(3).ref_cell(),
            Value::Int(9).ref_cell(),
        ])
        .ref_cell();
        let v = nimap_message_set(&[ids], span()).unwrap();
        let got = v.borrow().clone();
        assert!(values_equal(&got, &Value::String("1:3,9".into())));
    }

    #[test]
    fn parse_headers_doctest() {
        let raw = "Subject: Hi\r\n\r\n";
        let v = nimap_parse_headers(&[Value::String(raw.into()).ref_cell()], span()).unwrap();
        let got = v.borrow().clone();
        match got {
            Value::Object(m) => {
                let subj = m.get("subject").unwrap().borrow().clone();
                assert!(values_equal(&subj, &Value::String("Hi".into())));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn builtins_count() {
        assert_eq!(all_pairs().len(), 41);
    }

    #[test]
    fn arity_errors_throw() {
        let err = nimap_quote(&[], span()).unwrap_err();
        assert!(err.to_string().contains("expects 1"));
    }

    #[test]
    fn invalid_handle_is_value() {
        let v = nimap_logout(&[Value::Int(99999).ref_cell()], span()).unwrap();
        let got = v.borrow().clone();
        match got {
            Value::Error(e) => assert_eq!(e.code, E4534),
            other => panic!("expected error value, got {other:?}"),
        }
    }
}
