//! PostgreSQL wire protocol client (sync, TLS-free v1).

pub mod config;
pub mod error;
pub mod row;
pub mod tls;
pub mod types;

mod auth;
mod md5;
mod wire;

pub use config::{Config, SslMode};
pub use error::Error;
pub use row::Row;
pub use types::{FromSql, ToSql};

use std::io::Read;
use std::net::TcpStream;
use std::time::Duration;

pub struct Client {
    stream: TcpStream,
    read_buf: Vec<u8>,
    notifications: Vec<Notification>,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub channel: String,
    pub payload: String,
    pub pid: i32,
}

pub struct Transaction<'a> {
    client: &'a mut Client,
    open: bool,
}

pub struct CopyInWriter<'a> {
    client: &'a mut Client,
}

impl CopyInWriter<'_> {
    pub fn write(&mut self, data: &[u8]) -> Result<(), Error> {
        wire::write_copy_data(&mut self.client.stream, data).map_err(Error::io)
    }

    pub fn finish(self) -> Result<u64, Error> {
        self.client.finish_copy_in()
    }
}

impl Client {
    pub fn connect(config: &Config, _tls: tls::NoTls) -> Result<Self, Error> {
        if config.get_ssl_mode() != SslMode::Disable {
            return Err(Error::msg(
                "PostgreSQL SSL is not enabled in this build; use sslmode=disable",
            ));
        }
        let host = config.get_hosts().first().cloned().unwrap_or_else(|| "localhost".into());
        let port = config.get_ports().first().copied().unwrap_or(5432);
        let addr = format!("{host}:{port}");
        let timeout = config.get_connect_timeout().unwrap_or(Duration::from_secs(30));
        let stream = TcpStream::connect(&addr).map_err(Error::io)?;
        stream.set_read_timeout(Some(timeout)).map_err(Error::io)?;
        stream.set_write_timeout(Some(timeout)).map_err(Error::io)?;
        let mut client = Self {
            stream,
            read_buf: Vec::with_capacity(8192),
            notifications: Vec::new(),
        };
        client.startup(config)?;
        Ok(client)
    }

    pub fn batch_execute(&mut self, sql: &str) -> Result<(), Error> {
        wire::write_query(&mut self.stream, sql).map_err(Error::io)?;
        self.read_until_ready()
    }

    pub fn execute(&mut self, sql: &str, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error> {
        if params.is_empty() {
            wire::write_query(&mut self.stream, sql).map_err(Error::io)?;
            return self.read_command_complete();
        }
        self.extended(sql, params, false)
    }

    pub fn query(&mut self, sql: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Vec<Row>, Error> {
        if params.is_empty() {
            wire::write_query(&mut self.stream, sql).map_err(Error::io)?;
            return self.read_rows();
        }
        let n = self.extended(sql, params, true)?;
        if n == 0 {
            Ok(Vec::new())
        } else {
            self.read_rows()
        }
    }

    pub fn query_one(&mut self, sql: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Row, Error> {
        let rows = self.query(sql, params)?;
        rows.into_iter()
            .next()
            .ok_or_else(|| Error::msg("query returned no rows"))
    }

    pub fn transaction(&mut self) -> Result<Transaction<'_>, Error> {
        self.batch_execute("BEGIN")?;
        Ok(Transaction {
            client: self,
            open: true,
        })
    }

    pub fn check_query(&mut self, sql: &str) -> Result<(), Error> {
        wire::write_parse(&mut self.stream, "", sql).map_err(Error::io)?;
        wire::write_sync(&mut self.stream).map_err(Error::io)?;
        self.read_until_ready()
    }

    pub fn prepare(&mut self, sql: &str) -> Result<(), Error> {
        self.check_query(sql)
    }

    pub fn copy_in(&mut self, sql: &str) -> Result<CopyInWriter<'_>, Error> {
        wire::write_query(&mut self.stream, sql).map_err(Error::io)?;
        loop {
            let msg = self.read_message()?;
            match msg.tag {
                b'G' => {
                    return Ok(CopyInWriter { client: self });
                }
                b'E' => return Err(Error::from_error_response(&msg.body)),
                b'1' | b'2' | b'T' | b't' | b'C' | b'Z' | b'S' | b'K' | b'n' | b'N' => {}
                other => {
                    return Err(Error::msg(format!("unexpected message before COPY: {other}")));
                }
            }
        }
    }

    fn finish_copy_in(&mut self) -> Result<u64, Error> {
        wire::write_copy_done(&mut self.stream).map_err(Error::io)?;
        self.read_command_complete()
    }

    pub fn notifications(&mut self) -> Vec<Notification> {
        std::mem::take(&mut self.notifications)
    }

    fn startup(&mut self, config: &Config) -> Result<(), Error> {
        let mut params = Vec::new();
        if let Some(user) = config.get_user() {
            params.push(("user", user.to_string()));
        }
        if let Some(db) = config.get_dbname() {
            params.push(("database", db.to_string()));
        }
        if let Some(app) = config.get_application_name() {
            params.push(("application_name", app.to_string()));
        }
        wire::write_startup(&mut self.stream, &params).map_err(Error::io)?;
        loop {
            let msg = self.read_message()?;
            match msg.tag {
                b'R' => auth::handle_auth(&mut self.stream, &msg.body, config)?,
                b'K' => {}
                b'S' => {}
                b'Z' => {
                    let status = msg.body.first().copied().unwrap_or(b'I');
                    if status != b'I' {
                        return Err(Error::msg("server not ready after startup"));
                    }
                    return Ok(());
                }
                b'E' => return Err(Error::from_error_response(&msg.body)),
                b'N' => {}
                other => {
                    return Err(Error::msg(format!("unexpected startup message '{other}'")));
                }
            }
        }
    }

    fn extended(
        &mut self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
        want_rows: bool,
    ) -> Result<u64, Error> {
        wire::write_parse(&mut self.stream, "", sql).map_err(Error::io)?;
        let encoded: Vec<Option<String>> = params
            .iter()
            .map(|p| p.to_sql_opt())
            .collect::<Result<_, _>>()?;
        wire::write_bind(&mut self.stream, "", "", &encoded).map_err(Error::io)?;
        wire::write_execute(&mut self.stream, "", if want_rows { 0 } else { 1 }).map_err(Error::io)?;
        wire::write_sync(&mut self.stream).map_err(Error::io)?;
        if want_rows {
            Ok(0)
        } else {
            self.read_command_complete()
        }
    }

    fn read_command_complete(&mut self) -> Result<u64, Error> {
        let mut affected = 0u64;
        loop {
            let msg = self.read_message()?;
            match msg.tag {
                b'C' => {
                    affected = wire::parse_command_complete(&msg.body);
                }
                b'Z' => return Ok(affected),
                b'E' => return Err(Error::from_error_response(&msg.body)),
                b'1' | b'2' | b't' | b'T' | b'n' | b'N' | b'K' | b'S' => {}
                _ => {}
            }
        }
    }

    fn read_rows(&mut self) -> Result<Vec<Row>, Error> {
        let mut rows = Vec::new();
        let mut columns = Vec::new();
        loop {
            let msg = self.read_message()?;
            match msg.tag {
                b'T' => {
                    columns = wire::parse_row_description(&msg.body);
                }
                b'D' => rows.push(Row::from_data_row(&columns, &msg.body)),
                b'C' => {}
                b'Z' => return Ok(rows),
                b'E' => return Err(Error::from_error_response(&msg.body)),
                b'1' | b'2' | b't' | b'n' | b'N' | b'K' | b'S' => {}
                _ => {}
            }
        }
    }

    fn read_until_ready(&mut self) -> Result<(), Error> {
        loop {
            let msg = self.read_message()?;
            match msg.tag {
                b'Z' => return Ok(()),
                b'E' => return Err(Error::from_error_response(&msg.body)),
                _ => {}
            }
        }
    }

    fn read_message(&mut self) -> Result<wire::Message, Error> {
        loop {
            if let Some(msg) = wire::try_parse_message(&self.read_buf)? {
                let consumed = msg.consumed;
                let out = msg.message;
                self.read_buf.drain(..consumed);
                if out.tag == b'A' {
                    if let Some(n) = parse_notification(&out.body) {
                        self.notifications.push(n);
                    }
                    continue;
                }
                return Ok(out);
            }
            let mut tmp = [0u8; 4096];
            let n = self.stream.read(&mut tmp).map_err(Error::io)?;
            if n == 0 {
                return Err(Error::msg("connection closed"));
            }
            self.read_buf.extend_from_slice(&tmp[..n]);
        }
    }
}

impl<'a> Transaction<'a> {
    pub fn execute(&mut self, sql: &str, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error> {
        self.client.execute(sql, params)
    }

    pub fn commit(mut self) -> Result<(), Error> {
        self.open = false;
        self.client.batch_execute("COMMIT")
    }

    pub fn rollback(mut self) -> Result<(), Error> {
        self.open = false;
        self.client.batch_execute("ROLLBACK")
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if self.open {
            let _ = self.client.batch_execute("ROLLBACK");
        }
    }
}

fn parse_notification(body: &[u8]) -> Option<Notification> {
    if body.len() < 4 {
        return None;
    }
    let pid = i32::from_be_bytes(body[0..4].try_into().ok()?);
    let rest = &body[4..];
    let nul1 = rest.iter().position(|&b| b == 0)?;
    let channel = std::str::from_utf8(&rest[..nul1]).ok()?.to_string();
    let rest2 = &rest[nul1 + 1..];
    let nul2 = rest2.iter().position(|&b| b == 0)?;
    let payload = std::str::from_utf8(&rest2[..nul2]).ok()?.to_string();
    Some(Notification {
        channel,
        payload,
        pid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pg_url() -> Option<String> {
        std::env::var("NIAO_TEST_PG_URL").ok().filter(|s| !s.is_empty())
    }

    #[test]
    fn integration_select_one() {
        let Some(url) = pg_url() else { return };
        let config: Config = url.parse().unwrap();
        let mut client = Client::connect(&config, tls::NoTls).unwrap();
        let row = client.query_one("SELECT 1::int8, 'hi'::text", &[]).unwrap();
        assert_eq!(row.try_get::<_, i64>(0).unwrap(), 1);
        assert_eq!(row.try_get::<_, String>(1).unwrap(), "hi");
    }

    #[test]
    fn integration_parameterized() {
        let Some(url) = pg_url() else { return };
        let config: Config = url.parse().unwrap();
        let mut client = Client::connect(&config, tls::NoTls).unwrap();
        let n: i64 = 41;
        let row = client
            .query_one("SELECT $1::int4 + 1", &[&n])
            .unwrap();
        assert_eq!(row.try_get::<_, i32>(0).unwrap(), 42);
    }
}
