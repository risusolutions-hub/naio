//! Sync Redis client (RESP2).

use crate::resp::{encode_command, Reader, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

#[derive(Debug)]
pub struct RedisError(pub String);

impl std::fmt::Display for RedisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RedisError {}

pub struct Client {
    stream: TcpStream,
    read_buf: Vec<u8>,
}

impl Client {
    pub fn open(url: &str) -> Result<Self, RedisError> {
        let (host, port, password, db) = parse_url(url)?;
        let addr = format!("{host}:{port}");
        let stream = TcpStream::connect(&addr)
            .map_err(|e| RedisError(format!("connect: {e}")))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| RedisError(e.to_string()))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| RedisError(e.to_string()))?;
        let mut client = Self {
            stream,
            read_buf: Vec::with_capacity(4096),
        };
        if let Some(pw) = password {
            client.auth(&pw)?;
        }
        if db > 0 {
            client.select(db)?;
        }
        Ok(client)
    }

    pub fn ping(&mut self) -> Result<String, RedisError> {
        self.cmd_simple(&[b"PING"])
    }

    pub fn get(&mut self, key: &str) -> Result<Option<String>, RedisError> {
        let v = self.cmd(&[b"GET", key.as_bytes()])?;
        match v {
            Value::BulkString(Some(b)) => Ok(Some(String::from_utf8_lossy(&b).into_owned())),
            Value::BulkString(None) | Value::Null => Ok(None),
            other => Err(RedisError(format!("unexpected GET reply: {other:?}"))),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), RedisError> {
        self.cmd_ok(&[b"SET", key.as_bytes(), value.as_bytes()])
    }

    pub fn del(&mut self, key: &str) -> Result<(), RedisError> {
        self.cmd_ok(&[b"DEL", key.as_bytes()])
    }

    pub fn incr(&mut self, key: &str, by: i64) -> Result<i64, RedisError> {
        let by_s = by.to_string();
        let v = self.cmd(&[b"INCRBY", key.as_bytes(), by_s.as_bytes()])?;
        match v {
            Value::Integer(n) => Ok(n),
            other => Err(RedisError(format!("unexpected INCR reply: {other:?}"))),
        }
    }

    pub fn expire(&mut self, key: &str, secs: u64) -> Result<bool, RedisError> {
        let s = secs.to_string();
        let v = self.cmd(&[b"EXPIRE", key.as_bytes(), s.as_bytes()])?;
        match v {
            Value::Integer(n) => Ok(n == 1),
            other => Err(RedisError(format!("unexpected EXPIRE reply: {other:?}"))),
        }
    }

    fn auth(&mut self, password: &str) -> Result<(), RedisError> {
        self.cmd_ok(&[b"AUTH", password.as_bytes()])
    }

    fn select(&mut self, db: u32) -> Result<(), RedisError> {
        let s = db.to_string();
        self.cmd_ok(&[b"SELECT", s.as_bytes()])
    }

    fn cmd_ok(&mut self, parts: &[&[u8]]) -> Result<(), RedisError> {
        let _ = self.cmd_simple(parts)?;
        Ok(())
    }

    fn cmd_simple(&mut self, parts: &[&[u8]]) -> Result<String, RedisError> {
        match self.cmd(parts)? {
            Value::SimpleString(s) => Ok(s),
            Value::BulkString(Some(b)) => Ok(String::from_utf8_lossy(&b).into_owned()),
            Value::Error(e) => Err(RedisError(e)),
            other => Err(RedisError(format!("unexpected reply: {other:?}"))),
        }
    }

    fn cmd(&mut self, parts: &[&[u8]]) -> Result<Value, RedisError> {
        let enc = encode_command(parts);
        self.stream
            .write_all(&enc)
            .map_err(|e| RedisError(e.to_string()))?;
        self.read_buf.clear();
        let mut tmp = [0u8; 4096];
        loop {
            let n = self
                .stream
                .read(&mut tmp)
                .map_err(|e| RedisError(e.to_string()))?;
            if n == 0 {
                return Err(RedisError("connection closed".into()));
            }
            self.read_buf.extend_from_slice(&tmp[..n]);
            let mut reader = Reader::new(&self.read_buf);
            match reader.parse_one() {
                Ok(v) => return Ok(v),
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => continue,
                Err(e) => return Err(RedisError(e.to_string())),
            }
        }
    }
}

fn parse_url(url: &str) -> Result<(String, u16, Option<String>, u32), RedisError> {
    let rest = url
        .strip_prefix("redis://")
        .or_else(|| url.strip_prefix("rediss://"))
        .unwrap_or(url);
    let (auth_host, db) = match rest.rsplit_once('/') {
        Some((h, d)) if !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()) => {
            (h, d.parse().unwrap_or(0))
        }
        _ => (rest, 0),
    };
    let (auth, hostport) = match auth_host.rsplit_once('@') {
        Some((a, h)) => (Some(a.trim_start_matches(':').to_string()), h),
        None => (None, auth_host),
    };
    let password = auth.map(|a| a.to_string());
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(6379)),
        None => (hostport.to_string(), 6379),
    };
    Ok((host, port, password, db))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_redis_url() {
        let (h, p, pw, db) = parse_url("redis://:secret@127.0.0.1:6380/2").unwrap();
        assert_eq!(h, "127.0.0.1");
        assert_eq!(p, 6380);
        assert_eq!(pw.as_deref(), Some("secret"));
        assert_eq!(db, 2);
    }

    #[test]
    fn integration_redis() {
        let url = std::env::var("NIAO_TEST_REDIS_URL").unwrap_or_default();
        if url.is_empty() {
            return;
        }
        let mut c = Client::open(&url).unwrap();
        c.set("niao_db_test", "1").unwrap();
        assert_eq!(c.get("niao_db_test").unwrap().as_deref(), Some("1"));
        assert_eq!(c.incr("niao_db_test", 1).unwrap(), 2);
        c.del("niao_db_test").unwrap();
        assert!(c.ping().unwrap().eq_ignore_ascii_case("PONG"));
    }
}
