//! POP3 client (RFC 1939 + CAPA / UIDL). STARTTLS (STLS) supported via rustls.

use crate::error::{ImapError, Result};
use crate::wire::Conn;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PopConnectOptions {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
    pub tls: bool,
    pub starttls: bool,
    pub timeout: Duration,
}

impl PopConnectOptions {
    pub fn default_port(tls: bool) -> u16 {
        if tls {
            995
        } else {
            110
        }
    }
}

#[derive(Debug, Clone)]
pub struct PopStat {
    pub count: u32,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct PopListItem {
    pub msg: u32,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct PopUidlItem {
    pub msg: u32,
    pub uid: String,
}

pub struct PopClient {
    conn: Option<Conn>,
    pub host: String,
    pub port: u16,
}

impl PopClient {
    fn conn_mut(&mut self) -> Result<&mut Conn> {
        self.conn.as_mut().ok_or(ImapError::NotConnected)
    }

    pub fn connect(opts: &PopConnectOptions) -> Result<Self> {
        let mut conn = Conn::connect(&opts.host, opts.port, opts.timeout, opts.tls)?;
        let greet = conn.read_line()?;
        if !greet.starts_with("+OK") {
            return Err(ImapError::Protocol(format!("bad POP3 greeting: {greet}")));
        }

        if opts.starttls && !opts.tls {
            conn.write_line("STLS")?;
            let line = conn.read_line()?;
            if !line.starts_with("+OK") {
                return Err(ImapError::Protocol(line));
            }
            conn = conn.upgrade_tls(&opts.host)?;
        }

        let mut client = Self {
            conn: Some(conn),
            host: opts.host.clone(),
            port: opts.port,
        };

        client.cmd_ok(&format!("USER {}", opts.user))?;
        client.cmd_ok(&format!("PASS {}", opts.pass))?;
        Ok(client)
    }

    fn cmd_ok(&mut self, cmd: &str) -> Result<String> {
        self.conn_mut()?.write_line(cmd)?;
        let line = self.conn_mut()?.read_line()?;
        if line.starts_with("+OK") {
            Ok(line)
        } else {
            Err(ImapError::Protocol(line))
        }
    }

    fn cmd_multiline(&mut self, cmd: &str) -> Result<(String, Vec<String>)> {
        let hdr = self.cmd_ok(cmd)?;
        let mut lines = Vec::new();
        loop {
            let line = self.conn_mut()?.read_line()?;
            if line == "." {
                break;
            }
            let line = if let Some(rest) = line.strip_prefix("..") {
                format!(".{rest}")
            } else {
                line
            };
            lines.push(line);
        }
        Ok((hdr, lines))
    }

    pub fn capa(&mut self) -> Result<Vec<String>> {
        let (_, lines) = self.cmd_multiline("CAPA")?;
        Ok(lines)
    }

    pub fn stat(&mut self) -> Result<PopStat> {
        let line = self.cmd_ok("STAT")?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            Ok(PopStat {
                count: parts[1].parse().unwrap_or(0),
                size: parts[2].parse().unwrap_or(0),
            })
        } else {
            Err(ImapError::Protocol(line))
        }
    }

    pub fn list(&mut self, msg: Option<u32>) -> Result<Vec<PopListItem>> {
        match msg {
            Some(n) => {
                let line = self.cmd_ok(&format!("LIST {n}"))?;
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    Ok(vec![PopListItem {
                        msg: parts[1].parse().unwrap_or(n),
                        size: parts[2].parse().unwrap_or(0),
                    }])
                } else {
                    Err(ImapError::Protocol(line))
                }
            }
            None => {
                let (_, lines) = self.cmd_multiline("LIST")?;
                Ok(lines
                    .iter()
                    .filter_map(|l| {
                        let p: Vec<&str> = l.split_whitespace().collect();
                        if p.len() >= 2 {
                            Some(PopListItem {
                                msg: p[0].parse().ok()?,
                                size: p[1].parse().ok()?,
                            })
                        } else {
                            None
                        }
                    })
                    .collect())
            }
        }
    }

    pub fn retr(&mut self, msg: u32) -> Result<String> {
        let (_, lines) = self.cmd_multiline(&format!("RETR {msg}"))?;
        Ok(lines.join("\r\n"))
    }

    pub fn top(&mut self, msg: u32, lines_n: u32) -> Result<String> {
        let (_, lines) = self.cmd_multiline(&format!("TOP {msg} {lines_n}"))?;
        Ok(lines.join("\r\n"))
    }

    pub fn dele(&mut self, msg: u32) -> Result<()> {
        self.cmd_ok(&format!("DELE {msg}"))?;
        Ok(())
    }

    pub fn noop(&mut self) -> Result<()> {
        self.cmd_ok("NOOP")?;
        Ok(())
    }

    pub fn rset(&mut self) -> Result<()> {
        self.cmd_ok("RSET")?;
        Ok(())
    }

    pub fn uidl(&mut self, msg: Option<u32>) -> Result<Vec<PopUidlItem>> {
        match msg {
            Some(n) => {
                let line = self.cmd_ok(&format!("UIDL {n}"))?;
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    Ok(vec![PopUidlItem {
                        msg: parts[1].parse().unwrap_or(n),
                        uid: parts[2].to_string(),
                    }])
                } else {
                    Err(ImapError::Protocol(line))
                }
            }
            None => {
                let (_, lines) = self.cmd_multiline("UIDL")?;
                Ok(lines
                    .iter()
                    .filter_map(|l| {
                        let p: Vec<&str> = l.split_whitespace().collect();
                        if p.len() >= 2 {
                            Some(PopUidlItem {
                                msg: p[0].parse().ok()?,
                                uid: p[1].to_string(),
                            })
                        } else {
                            None
                        }
                    })
                    .collect())
            }
        }
    }

    pub fn quit(&mut self) -> Result<()> {
        self.cmd_ok("QUIT")?;
        self.conn = None;
        Ok(())
    }
}
