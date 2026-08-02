//! Connection string parsing and client open helpers.

use mysql::{Conn, Opts, OptsBuilder};
use niao_db::ManageConnection;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use super::handles::redact_conninfo;
use crate::{RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;

#[derive(Clone)]
pub struct MysqlConnectionManager {
    pub opts: Opts,
    pub reconnect_url: String,
}

impl ManageConnection for MysqlConnectionManager {
    type Connection = Conn;
    fn connect(&self) -> Result<Conn, String> {
        Conn::new(self.opts.clone()).map_err(|e| e.to_string())
    }
}

pub type MysqlPool = Arc<niao_db::Pool<MysqlConnectionManager>>;

pub fn connect_opts_raw(opts: Opts) -> Result<Conn, String> {
    Conn::new(opts).map_err(|e| e.to_string())
}

pub fn connect_url(url: &str) -> Result<(Conn, Opts, String), String> {
    let opts = Opts::from_url(url).map_err(|e| e.to_string())?;
    if opts.get_ssl_opts().is_some() {
        // mysql crate enables SSL only when SslOpts is set; plain URLs are fine.
    }
    let conn = Conn::new(opts.clone()).map_err(|e| e.to_string())?;
    Ok((conn, opts, url.to_string()))
}

fn sslmode_ok(mode: &str) -> Result<(), String> {
    match mode.to_lowercase().as_str() {
        "disable" | "" => Ok(()),
        other => Err(format!(
            "MySQL SSL mode \"{other}\" is not enabled in this build; use sslmode=disable"
        )),
    }
}

pub fn config_from_opts(
    opts_map: &HashMap<String, ValueRef>,
) -> Result<(Opts, String, String), String> {
    if let Some(url_ref) = opts_map.get("url").or_else(|| opts_map.get("connection_string")) {
        let url = match &*url_ref.borrow() {
            Value::String(s) => s.clone(),
            other => return Err(format!("url must be string, got {}", other.type_name())),
        };
        if let Some(ssl_ref) = opts_map.get("sslmode") {
            if let Value::String(s) = &*ssl_ref.borrow() {
                sslmode_ok(s)?;
            }
        }
        let opts = Opts::from_url(&url).map_err(|e| e.to_string())?;
        return Ok((opts, url.clone(), redact_conninfo(&url)));
    }

    let mut builder = OptsBuilder::new();
    let mut display_parts = Vec::new();

    let host = opts_map
        .get("host")
        .map(|v| match &*v.borrow() {
            Value::String(s) => Ok(s.clone()),
            other => Err(format!("host must be string, got {}", other.type_name())),
        })
        .transpose()?
        .unwrap_or_else(|| "localhost".to_string());
    builder = builder.ip_or_hostname(Some(host.clone()));
    display_parts.push(format!("host={host}"));

    let port = opts_map
        .get("port")
        .map(|v| match &*v.borrow() {
            Value::Int(n) => Ok(*n as u16),
            other => Err(format!("port must be int, got {}", other.type_name())),
        })
        .transpose()?
        .unwrap_or(3306);
    builder = builder.tcp_port(port);
    display_parts.push(format!("port={port}"));

    let user = opts_map
        .get("user")
        .map(|v| match &*v.borrow() {
            Value::String(s) => Ok(s.clone()),
            other => Err(format!("user must be string, got {}", other.type_name())),
        })
        .transpose()?
        .unwrap_or_else(|| "root".to_string());
    builder = builder.user(Some(user.clone()));
    display_parts.push(format!("user={user}"));

    let password = opts_map
        .get("password")
        .map(|v| match &*v.borrow() {
            Value::String(s) => Ok(s.clone()),
            other => Err(format!("password must be string, got {}", other.type_name())),
        })
        .transpose()?
        .unwrap_or_default();
    builder = builder.pass(Some(password.clone()));
    display_parts.push("password=***".to_string());

    let db = opts_map
        .get("database")
        .or_else(|| opts_map.get("db"))
        .map(|v| match &*v.borrow() {
            Value::String(s) => Ok(s.clone()),
            other => Err(format!("database must be string, got {}", other.type_name())),
        })
        .transpose()?;
    if let Some(ref dbname) = db {
        builder = builder.db_name(Some(dbname.clone()));
        display_parts.push(format!("database={dbname}"));
    }

    if let Some(ssl_ref) = opts_map.get("sslmode") {
        let mode = match &*ssl_ref.borrow() {
            Value::String(s) => s.clone(),
            other => return Err(format!("sslmode must be string, got {}", other.type_name())),
        };
        sslmode_ok(&mode)?;
        display_parts.push(format!("sslmode={mode}"));
    } else {
        display_parts.push("sslmode=disable".to_string());
    }

    if let Some(ct_ref) = opts_map.get("connect_timeout") {
        let secs = match &*ct_ref.borrow() {
            Value::Int(n) if *n > 0 => *n as u64,
            other => {
                return Err(format!(
                    "connect_timeout must be positive int, got {}",
                    other.type_name()
                ));
            }
        };
        builder = builder.tcp_connect_timeout(Some(Duration::from_secs(secs)));
        display_parts.push(format!("connect_timeout={secs}"));
    }

    let opts = Opts::from(builder);

    // Build reconnect URL for async workers.
    let user_enc = urlencoding_simple(&user);
    let pass_enc = urlencoding_simple(&password);
    let db_path = db
        .as_ref()
        .map(|d| format!("/{}", urlencoding_simple(d)))
        .unwrap_or_default();
    let reconnect_url = format!("mysql://{user_enc}:{pass_enc}@{host}:{port}{db_path}");

    Ok((opts, reconnect_url, display_parts.join(" ")))
}

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn parse_connect_opts(opts_ref: &ValueRef, span: Span) -> Result<(Opts, String, String), RuntimeError> {
    let opts = match &*opts_ref.borrow() {
        Value::Object(map) => map.clone(),
        other => {
            return Err(RuntimeError::at(
                span,
                codes::E1910_NMYSQL_ARITY,
                format!(
                    "nmysql.connect_opts() expects options object, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    config_from_opts(&opts).map_err(|msg| RuntimeError::at(span, codes::E1917_NMYSQL_TLS, msg))
}

pub fn pool_manager(opts: Opts, reconnect_url: String) -> MysqlConnectionManager {
    MysqlConnectionManager { opts, reconnect_url }
}

pub fn pool_opts_from_map(
    opts_map: &HashMap<String, ValueRef>,
) -> Result<(Opts, String, String, u32, u32, Option<Duration>, Duration), String> {
    let (config, reconnect, display) = config_from_opts(opts_map)?;
    let max_size = opts_map
        .get("max_size")
        .map(|v| match &*v.borrow() {
            Value::Int(n) if *n > 0 => Ok(*n as u32),
            other => Err(format!("max_size must be positive int, got {}", other.type_name())),
        })
        .transpose()?
        .unwrap_or(10);
    let min_idle = opts_map
        .get("min_idle")
        .map(|v| match &*v.borrow() {
            Value::Int(n) if *n >= 0 => Ok(*n as u32),
            other => Err(format!("min_idle must be non-negative int, got {}", other.type_name())),
        })
        .transpose()?
        .unwrap_or(0);
    let max_lifetime = opts_map
        .get("max_lifetime_secs")
        .map(|v| match &*v.borrow() {
            Value::Int(n) if *n > 0 => Ok(Duration::from_secs(*n as u64)),
            other => Err(format!(
                "max_lifetime_secs must be positive int, got {}",
                other.type_name()
            )),
        })
        .transpose()?;
    let connection_timeout = opts_map
        .get("connection_timeout_secs")
        .map(|v| match &*v.borrow() {
            Value::Int(n) if *n > 0 => Ok(Duration::from_secs(*n as u64)),
            other => Err(format!(
                "connection_timeout_secs must be positive int, got {}",
                other.type_name()
            )),
        })
        .transpose()?
        .unwrap_or(Duration::from_secs(30));
    Ok((
        config,
        reconnect,
        display,
        max_size,
        min_idle,
        max_lifetime,
        connection_timeout,
    ))
}
