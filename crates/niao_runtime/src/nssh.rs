//! Native `nssh` standard library — SSH client (~paramiko, fabric).
//!
//! Import with `import "nssh"` (or `import "std/nssh"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_ssh::{
    agent_identities, close, connect, exec, forward_close, forward_local, is_connected,
    key_fingerprint, sftp_close, sftp_get, sftp_listdir, sftp_mkdir, sftp_open, sftp_put, sftp_read,
    sftp_remove, sftp_rename, sftp_rmdir, sftp_stat, sftp_write, shell_close, shell_open, shell_read,
    shell_write, ConnectConfig, SshError,
};
use std::collections::HashMap;
use std::rc::Rc;

const E3600_NSSH_ARITY: u32 = codes::E3600_NSSH_ARITY;
const E3601_NSSH_ERROR: u32 = codes::E3601_NSSH_ERROR;
const E3602_NSSH_TYPE: u32 = codes::E3602_NSSH_TYPE;
const E3603_NSSH_INVALID_HANDLE: u32 = codes::E3603_NSSH_INVALID_HANDLE;
const E3604_NSSH_AUTH: u32 = codes::E3604_NSSH_AUTH;

fn nssh_err(span: Span, e: SshError) -> ValueRef {
    let code = match &e {
        SshError::InvalidHandle(_) => E3603_NSSH_INVALID_HANDLE,
        SshError::AuthFailed => E3604_NSSH_AUTH,
        _ => E3601_NSSH_ERROR,
    };
    error_value(code, "nssh_error", e.to_string(), span)
}

fn map_res(span: Span, r: Result<ValueRef, SshError>) -> NiaoResult<ValueRef> {
    match r {
        Ok(v) => Ok(v),
        Err(e) => Ok(nssh_err(span, e)),
    }
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3600_NSSH_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3600_NSSH_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3602_NSSH_TYPE, msg.into())
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects bytes or string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn opt_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::Object(m) => Some(m.clone()),
        _ => None,
    })
}

fn map_get_str(m: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    m.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn map_get_int(m: &HashMap<String, ValueRef>, key: &str) -> Option<i64> {
    m.get(key).and_then(|v| match &*v.borrow() {
        Value::Int(n) => Some(*n),
        _ => None,
    })
}

fn map_get_bool(m: &HashMap<String, ValueRef>, key: &str) -> Option<bool> {
    m.get(key).and_then(|v| match &*v.borrow() {
        Value::Bool(b) => Some(*b),
        _ => None,
    })
}

fn parse_connect(cfg: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<ConnectConfig> {
    let host = map_get_str(cfg, "host").ok_or_else(|| type_err(span, "connect() requires host"))?;
    let user = map_get_str(cfg, "user").ok_or_else(|| type_err(span, "connect() requires user"))?;
    let mut c = ConnectConfig::new(host, user);
    if let Some(p) = map_get_int(cfg, "port") {
        if !(0..=65535).contains(&p) {
            return Err(type_err(span, "connect() port must be 0..=65535"));
        }
        c.port = p as u16;
    }
    c.password = map_get_str(cfg, "password");
    c.key_path = map_get_str(cfg, "key").or_else(|| map_get_str(cfg, "key_path"));
    c.key_data = map_get_str(cfg, "key_data");
    c.passphrase = map_get_str(cfg, "passphrase");
    c.agent = map_get_bool(cfg, "agent").unwrap_or(false);
    if let Some(ms) = map_get_int(cfg, "timeout_ms") {
        if ms < 0 {
            return Err(type_err(span, "connect() timeout_ms must be >= 0"));
        }
        c.timeout_ms = Some(ms as u64);
    }
    Ok(c)
}

fn ok_true() -> ValueRef {
    Value::Bool(true).ref_cell()
}

fn bytes_val(b: Vec<u8>) -> ValueRef {
    Value::ByteArray(b).ref_cell()
}

fn string_lossy(b: &[u8]) -> ValueRef {
    Value::String(String::from_utf8_lossy(b).into_owned()).ref_cell()
}

// >>> nssh.connect({host: "127.0.0.1", user: "u", password: "p", port: 22})
// => session handle int
fn nssh_connect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nssh_connect", span)?;
    let map = match &*args[0].borrow() {
        Value::Object(m) => m.clone(),
        other => {
            return Err(type_err(
                span,
                format!("nssh_connect() expects a config object, got {}", other.type_name()),
            ))
        }
    };
    let cfg = parse_connect(&map, span)?;
    map_res(span, connect(&cfg).map(|id| Value::Int(id).ref_cell()))
}

// >>> nssh.close(session)
// => true
fn nssh_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nssh_close", span)?;
    let id = int_arg(args, 0, "nssh_close", span)?;
    map_res(span, close(id).map(|_| ok_true()))
}

// >>> nssh.is_connected(session)
// => true or false
fn nssh_is_connected(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nssh_is_connected", span)?;
    let id = int_arg(args, 0, "nssh_is_connected", span)?;
    Ok(Value::Bool(is_connected(id)).ref_cell())
}

// >>> nssh.exec(session, "echo hi")
// => {stdout, stderr, exit_status, ok}
fn nssh_exec(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nssh_exec", span)?;
    let id = int_arg(args, 0, "nssh_exec", span)?;
    let cmd = string_arg(args, 1, "nssh_exec", span)?;
    let timeout = opt_object(args, 2)
        .as_ref()
        .and_then(|m| map_get_int(m, "timeout_ms"))
        .map(|n| n.max(0) as u64);
    map_res(
        span,
        exec(id, &cmd, timeout).map(|r| {
            let mut o = HashMap::new();
            o.insert("stdout".into(), string_lossy(&r.stdout));
            o.insert("stderr".into(), string_lossy(&r.stderr));
            o.insert("stdout_bytes".into(), bytes_val(r.stdout.clone()));
            o.insert("stderr_bytes".into(), bytes_val(r.stderr.clone()));
            o.insert(
                "exit_status".into(),
                Value::Int(r.exit_status as i64).ref_cell(),
            );
            o.insert("ok".into(), Value::Bool(r.ok).ref_cell());
            Value::Object(o).ref_cell()
        }),
    )
}

// >>> nssh.shell(session)
// => channel handle
fn nssh_shell(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nssh_shell", span)?;
    let id = int_arg(args, 0, "nssh_shell", span)?;
    let opts = opt_object(args, 1);
    let term = opts
        .as_ref()
        .and_then(|m| map_get_str(m, "term"))
        .unwrap_or_else(|| "xterm".into());
    let cols = opts
        .as_ref()
        .and_then(|m| map_get_int(m, "cols"))
        .unwrap_or(80)
        .max(1) as u32;
    let rows = opts
        .as_ref()
        .and_then(|m| map_get_int(m, "rows"))
        .unwrap_or(24)
        .max(1) as u32;
    map_res(
        span,
        shell_open(id, &term, cols, rows).map(|c| Value::Int(c).ref_cell()),
    )
}

// >>> nssh.shell_write(channel, "ls\n")
// => true
fn nssh_shell_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nssh_shell_write", span)?;
    let id = int_arg(args, 0, "nssh_shell_write", span)?;
    let data = bytes_arg(args, 1, "nssh_shell_write", span)?;
    map_res(span, shell_write(id, &data).map(|_| ok_true()))
}

// >>> nssh.shell_read(channel)
// => string or nil
fn nssh_shell_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nssh_shell_read", span)?;
    let id = int_arg(args, 0, "nssh_shell_read", span)?;
    let opts = opt_object(args, 1);
    let timeout = opts
        .as_ref()
        .and_then(|m| map_get_int(m, "timeout_ms"))
        .map(|n| n.max(0) as u64);
    let max_bytes = opts
        .as_ref()
        .and_then(|m| map_get_int(m, "max_bytes"))
        .unwrap_or(65536)
        .max(1) as usize;
    map_res(
        span,
        shell_read(id, timeout, max_bytes).map(|o: Option<Vec<u8>>| match o {
            None => Value::Nil.ref_cell(),
            Some(b) => string_lossy(b.as_slice()),
        }),
    )
}

// >>> nssh.shell_close(channel)
// => true
fn nssh_shell_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nssh_shell_close", span)?;
    let id = int_arg(args, 0, "nssh_shell_close", span)?;
    map_res(span, shell_close(id).map(|_| ok_true()))
}

// >>> nssh.sftp_open(session)
// => sftp handle
fn nssh_sftp_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nssh_sftp_open", span)?;
    let id = int_arg(args, 0, "nssh_sftp_open", span)?;
    map_res(span, sftp_open(id).map(|h| Value::Int(h).ref_cell()))
}

fn nssh_sftp_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nssh_sftp_close", span)?;
    let id = int_arg(args, 0, "nssh_sftp_close", span)?;
    map_res(span, sftp_close(id).map(|_| ok_true()))
}

fn nssh_sftp_listdir(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nssh_sftp_listdir", span)?;
    let id = int_arg(args, 0, "nssh_sftp_listdir", span)?;
    let path = string_arg(args, 1, "nssh_sftp_listdir", span)?;
    map_res(
        span,
        sftp_listdir(id, &path).map(|entries| {
            Value::Array(
                entries
                    .into_iter()
                    .map(|e| {
                        let mut o = HashMap::new();
                        o.insert("name".into(), Value::String(e.name).ref_cell());
                        o.insert("size".into(), Value::Int(e.size as i64).ref_cell());
                        o.insert("is_dir".into(), Value::Bool(e.is_dir).ref_cell());
                        o.insert("is_file".into(), Value::Bool(e.is_file).ref_cell());
                        Value::Object(o).ref_cell()
                    })
                    .collect(),
            )
            .ref_cell()
        }),
    )
}

fn nssh_sftp_stat(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nssh_sftp_stat", span)?;
    let id = int_arg(args, 0, "nssh_sftp_stat", span)?;
    let path = string_arg(args, 1, "nssh_sftp_stat", span)?;
    map_res(
        span,
        sftp_stat(id, &path).map(|st| {
            let mut o = HashMap::new();
            o.insert("size".into(), Value::Int(st.size as i64).ref_cell());
            o.insert("is_dir".into(), Value::Bool(st.is_dir).ref_cell());
            o.insert("is_file".into(), Value::Bool(st.is_file).ref_cell());
            if let Some(p) = st.permissions {
                o.insert("permissions".into(), Value::Int(p as i64).ref_cell());
            }
            Value::Object(o).ref_cell()
        }),
    )
}

fn nssh_sftp_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nssh_sftp_read", span)?;
    let id = int_arg(args, 0, "nssh_sftp_read", span)?;
    let path = string_arg(args, 1, "nssh_sftp_read", span)?;
    map_res(span, sftp_read(id, &path).map(bytes_val))
}

fn nssh_sftp_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nssh_sftp_write", span)?;
    let id = int_arg(args, 0, "nssh_sftp_write", span)?;
    let path = string_arg(args, 1, "nssh_sftp_write", span)?;
    let data = bytes_arg(args, 2, "nssh_sftp_write", span)?;
    map_res(span, sftp_write(id, &path, &data).map(|_| ok_true()))
}

fn nssh_sftp_mkdir(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nssh_sftp_mkdir", span)?;
    let id = int_arg(args, 0, "nssh_sftp_mkdir", span)?;
    let path = string_arg(args, 1, "nssh_sftp_mkdir", span)?;
    map_res(span, sftp_mkdir(id, &path).map(|_| ok_true()))
}

fn nssh_sftp_rmdir(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nssh_sftp_rmdir", span)?;
    let id = int_arg(args, 0, "nssh_sftp_rmdir", span)?;
    let path = string_arg(args, 1, "nssh_sftp_rmdir", span)?;
    map_res(span, sftp_rmdir(id, &path).map(|_| ok_true()))
}

fn nssh_sftp_remove(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nssh_sftp_remove", span)?;
    let id = int_arg(args, 0, "nssh_sftp_remove", span)?;
    let path = string_arg(args, 1, "nssh_sftp_remove", span)?;
    map_res(span, sftp_remove(id, &path).map(|_| ok_true()))
}

fn nssh_sftp_rename(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nssh_sftp_rename", span)?;
    let id = int_arg(args, 0, "nssh_sftp_rename", span)?;
    let src = string_arg(args, 1, "nssh_sftp_rename", span)?;
    let dst = string_arg(args, 2, "nssh_sftp_rename", span)?;
    map_res(span, sftp_rename(id, &src, &dst).map(|_| ok_true()))
}

fn nssh_sftp_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nssh_sftp_get", span)?;
    let id = int_arg(args, 0, "nssh_sftp_get", span)?;
    let remote = string_arg(args, 1, "nssh_sftp_get", span)?;
    let local = string_arg(args, 2, "nssh_sftp_get", span)?;
    map_res(span, sftp_get(id, &remote, &local).map(|_| ok_true()))
}

fn nssh_sftp_put(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nssh_sftp_put", span)?;
    let id = int_arg(args, 0, "nssh_sftp_put", span)?;
    let local = string_arg(args, 1, "nssh_sftp_put", span)?;
    let remote = string_arg(args, 2, "nssh_sftp_put", span)?;
    map_res(span, sftp_put(id, &local, &remote).map(|_| ok_true()))
}

// >>> nssh.forward_local(session, 0, "127.0.0.1", 80)
// => {id, bind_port, bind_addr}
fn nssh_forward_local(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "nssh_forward_local", span)?;
    let id = int_arg(args, 0, "nssh_forward_local", span)?;
    let bind = int_arg(args, 1, "nssh_forward_local", span)?;
    let remote_host = string_arg(args, 2, "nssh_forward_local", span)?;
    let remote_port = int_arg(args, 3, "nssh_forward_local", span)?;
    if !(0..=65535).contains(&bind) || !(0..=65535).contains(&remote_port) {
        return Err(type_err(span, "nssh_forward_local() ports must be 0..=65535"));
    }
    map_res(
        span,
        forward_local(id, bind as u16, &remote_host, remote_port as u16).map(|f| {
            let mut o = HashMap::new();
            o.insert("id".into(), Value::Int(f.id).ref_cell());
            o.insert("bind_port".into(), Value::Int(f.bind_port as i64).ref_cell());
            o.insert("bind_addr".into(), Value::String(f.bind_addr).ref_cell());
            Value::Object(o).ref_cell()
        }),
    )
}

fn nssh_forward_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nssh_forward_close", span)?;
    let id = int_arg(args, 0, "nssh_forward_close", span)?;
    map_res(span, forward_close(id).map(|_| ok_true()))
}

// >>> nssh.agent_identities()
// => [{fingerprint, algorithm, comment}, ...]
fn nssh_agent_identities(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nssh_agent_identities", span)?;
    map_res(
        span,
        agent_identities().map(|ids| {
            Value::Array(
                ids.into_iter()
                    .map(|i| {
                        let mut o = HashMap::new();
                        o.insert("fingerprint".into(), Value::String(i.fingerprint).ref_cell());
                        o.insert("algorithm".into(), Value::String(i.algorithm).ref_cell());
                        o.insert("comment".into(), Value::String(i.comment).ref_cell());
                        Value::Object(o).ref_cell()
                    })
                    .collect(),
            )
            .ref_cell()
        }),
    )
}

// >>> nssh.key_fingerprint("~/.ssh/id_ed25519")
// => "SHA256:..."
fn nssh_key_fingerprint(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nssh_key_fingerprint", span)?;
    let path_or = string_arg(args, 0, "nssh_key_fingerprint", span)?;
    let opts = opt_object(args, 1);
    let is_path = opts
        .as_ref()
        .and_then(|m| map_get_bool(m, "pem"))
        .map(|pem| !pem)
        .unwrap_or(true);
    let pass = opts.as_ref().and_then(|m| map_get_str(m, "passphrase"));
    map_res(
        span,
        key_fingerprint(&path_or, is_path, pass.as_deref())
            .map(|s| Value::String(s).ref_cell()),
    )
}

macro_rules! nssh_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nssh_fns![
    ("nssh_connect", "connect", nssh_connect),
    ("nssh_close", "close", nssh_close),
    ("nssh_is_connected", "is_connected", nssh_is_connected),
    ("nssh_exec", "exec", nssh_exec),
    ("nssh_shell", "shell", nssh_shell),
    ("nssh_shell_write", "shell_write", nssh_shell_write),
    ("nssh_shell_read", "shell_read", nssh_shell_read),
    ("nssh_shell_close", "shell_close", nssh_shell_close),
    ("nssh_sftp_open", "sftp_open", nssh_sftp_open),
    ("nssh_sftp_close", "sftp_close", nssh_sftp_close),
    ("nssh_sftp_listdir", "sftp_listdir", nssh_sftp_listdir),
    ("nssh_sftp_stat", "sftp_stat", nssh_sftp_stat),
    ("nssh_sftp_read", "sftp_read", nssh_sftp_read),
    ("nssh_sftp_write", "sftp_write", nssh_sftp_write),
    ("nssh_sftp_mkdir", "sftp_mkdir", nssh_sftp_mkdir),
    ("nssh_sftp_rmdir", "sftp_rmdir", nssh_sftp_rmdir),
    ("nssh_sftp_remove", "sftp_remove", nssh_sftp_remove),
    ("nssh_sftp_rename", "sftp_rename", nssh_sftp_rename),
    ("nssh_sftp_get", "sftp_get", nssh_sftp_get),
    ("nssh_sftp_put", "sftp_put", nssh_sftp_put),
    ("nssh_forward_local", "forward_local", nssh_forward_local),
    ("nssh_forward_close", "forward_close", nssh_forward_close),
    ("nssh_agent_identities", "agent_identities", nssh_agent_identities),
    ("nssh_key_fingerprint", "key_fingerprint", nssh_key_fingerprint),
];

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nssh";
pub const MODULE_PATHS: &[&str] = &["nssh", "std/nssh"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn connect_arity() {
        let err = nssh_connect(&[], span()).unwrap_err();
        assert_eq!(err.code(), E3600_NSSH_ARITY);
    }

    #[test]
    fn connect_missing_host() {
        let err = nssh_connect(&[Value::Object(HashMap::new()).ref_cell()], span()).unwrap_err();
        assert_eq!(err.code(), E3602_NSSH_TYPE);
    }

    #[test]
    fn exec_invalid_handle() {
        let v = nssh_exec(
            &[
                Value::Int(999_999).ref_cell(),
                Value::String("true".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }
}
