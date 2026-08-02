//! FTP client via `niao_net_clients`.

use super::{net_error, ok_nil, ok_string, string_arg, NetResult};
use niao_ast::Span;
use niao_errors::codes;
use niao_net_clients::ftp::{connect, FtpClient};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static FTP_HANDLES: RefCell<HashMap<u64, FtpClient>> = RefCell::new(HashMap::new());
    static NEXT_FTP: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
}

fn alloc_ftp(client: FtpClient) -> u64 {
    let id = NEXT_FTP.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    FTP_HANDLES.with(|m| m.borrow_mut().insert(id, client));
    id
}

fn ftp_arg(
    args: &[crate::ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> Result<u64, crate::RuntimeError> {
    let id = super::handle_arg(args, idx, name, span)?;
    FTP_HANDLES.with(|m| {
        if m.borrow().contains_key(&id) {
            Ok(id)
        } else {
            Err(crate::RuntimeError::at(
                span,
                codes::E1402_NET_INVALID_HANDLE,
                format!("{name}(): invalid ftp handle {id}"),
            ))
        }
    })
}

pub fn net_ftp_connect(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity(args, 2, "net_ftp_connect", span)?;
    let host = string_arg(args, 0, "net_ftp_connect", span)?;
    let port = super::port_arg(args, 1, "net_ftp_connect", span)?;
    match connect(&host, port as u16) {
        Ok(client) => Ok(crate::Value::Int(alloc_ftp(client) as i64).ref_cell()),
        Err(e) => Ok(net_error(
            span,
            codes::E1401_NET_ERROR,
            "net_error",
            e.to_string(),
        )),
    }
}

pub fn net_ftp_login(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity(args, 3, "net_ftp_login", span)?;
    let id = ftp_arg(args, 0, "net_ftp_login", span)?;
    let user = string_arg(args, 1, "net_ftp_login", span)?;
    let pass = string_arg(args, 2, "net_ftp_login", span)?;
    FTP_HANDLES.with(|m| {
        let mut guard = m.borrow_mut();
        let client = guard.get_mut(&id).ok_or_else(|| {
            crate::RuntimeError::at(span, codes::E1402_NET_INVALID_HANDLE, "invalid ftp handle")
        })?;
        client
            .login(&user, &pass)
            .map_err(|e| crate::RuntimeError::at(span, codes::E1401_NET_ERROR, e.to_string()))?;
        Ok(ok_nil())
    })
}

pub fn net_ftp_get(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity(args, 2, "net_ftp_get", span)?;
    let id = ftp_arg(args, 0, "net_ftp_get", span)?;
    let remote = string_arg(args, 1, "net_ftp_get", span)?;
    FTP_HANDLES.with(|m| {
        let mut guard = m.borrow_mut();
        let client = guard.get_mut(&id).ok_or_else(|| {
            crate::RuntimeError::at(span, codes::E1402_NET_INVALID_HANDLE, "invalid ftp handle")
        })?;
        let data = client
            .get(&remote)
            .map_err(|e| crate::RuntimeError::at(span, codes::E1401_NET_ERROR, e.to_string()))?;
        Ok(ok_string(String::from_utf8_lossy(&data).into_owned()))
    })
}

pub fn net_ftp_put(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity(args, 3, "net_ftp_put", span)?;
    let id = ftp_arg(args, 0, "net_ftp_put", span)?;
    let remote = string_arg(args, 1, "net_ftp_put", span)?;
    let content = string_arg(args, 2, "net_ftp_put", span)?;
    FTP_HANDLES.with(|m| {
        let mut guard = m.borrow_mut();
        let client = guard.get_mut(&id).ok_or_else(|| {
            crate::RuntimeError::at(span, codes::E1402_NET_INVALID_HANDLE, "invalid ftp handle")
        })?;
        client
            .put(&remote, content.as_bytes())
            .map_err(|e| crate::RuntimeError::at(span, codes::E1401_NET_ERROR, e.to_string()))?;
        Ok(ok_nil())
    })
}

pub fn net_ftp_close(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity(args, 1, "net_ftp_close", span)?;
    let id = ftp_arg(args, 0, "net_ftp_close", span)?;
    FTP_HANDLES.with(|m| {
        if let Some(mut client) = m.borrow_mut().remove(&id) {
            let _ = client.quit();
        }
        Ok(ok_nil())
    })
}
