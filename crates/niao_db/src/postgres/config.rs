//! Connection configuration (postgres:// URL + builder).

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SslMode {
    Disable,
    Prefer,
    Require,
}

impl fmt::Display for SslMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SslMode::Disable => write!(f, "disable"),
            SslMode::Prefer => write!(f, "prefer"),
            SslMode::Require => write!(f, "require"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    hosts: Vec<String>,
    ports: Vec<u16>,
    user: Option<String>,
    password: Option<String>,
    dbname: Option<String>,
    ssl_mode: SslMode,
    connect_timeout: Option<Duration>,
    application_name: Option<String>,
}

impl Config {
    pub fn new() -> Self {
        Self {
            hosts: vec!["localhost".into()],
            ports: vec![5432],
            user: None,
            password: None,
            dbname: None,
            ssl_mode: SslMode::Disable,
            connect_timeout: None,
            application_name: None,
        }
    }

    pub fn host(&mut self, h: &str) -> &mut Self {
        self.hosts = vec![h.to_string()];
        self
    }

    pub fn port(&mut self, p: u16) -> &mut Self {
        self.ports = vec![p];
        self
    }

    pub fn user(&mut self, u: &str) -> &mut Self {
        self.user = Some(u.to_string());
        self
    }

    pub fn password(&mut self, p: &str) -> &mut Self {
        self.password = Some(p.to_string());
        self
    }

    pub fn dbname(&mut self, d: &str) -> &mut Self {
        self.dbname = Some(d.to_string());
        self
    }

    pub fn ssl_mode(&mut self, m: SslMode) -> &mut Self {
        self.ssl_mode = m;
        self
    }

    pub fn connect_timeout(&mut self, d: Duration) -> &mut Self {
        self.connect_timeout = Some(d);
        self
    }

    pub fn application_name(&mut self, n: &str) -> &mut Self {
        self.application_name = Some(n.to_string());
        self
    }

    pub fn get_hosts(&self) -> &[String] {
        &self.hosts
    }

    pub fn get_ports(&self) -> &[u16] {
        &self.ports
    }

    pub fn get_user(&self) -> Option<&str> {
        self.user.as_deref()
    }

    pub fn get_password(&self) -> Option<&str> {
        self.password.as_deref()
    }

    pub fn get_dbname(&self) -> Option<&str> {
        self.dbname.as_deref()
    }

    pub fn get_ssl_mode(&self) -> SslMode {
        self.ssl_mode
    }

    pub fn get_connect_timeout(&self) -> Option<Duration> {
        self.connect_timeout
    }

    pub fn get_application_name(&self) -> Option<&str> {
        self.application_name.as_deref()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

impl FromStr for Config {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_url(s)
    }
}

pub fn parse_url(url: &str) -> Result<Config, String> {
    let rest = url
        .strip_prefix("postgres://")
        .or_else(|| url.strip_prefix("postgresql://"))
        .ok_or_else(|| "expected postgres:// URL".to_string())?;
    let (auth, hostpart) = match rest.split_once('@') {
        Some((a, h)) => (Some(a), h),
        None => (None, rest),
    };
    let (hostport, query) = match hostpart.split_once('?') {
        Some((h, q)) => (h, Some(q)),
        None => (hostpart, None),
    };
    let (host, port, path) = parse_hostport_path(hostport)?;
    let mut cfg = Config::new();
    cfg.host(&host);
    cfg.port(port);
    if let Some(db) = path {
        cfg.dbname(&db);
    }
    if let Some(a) = auth {
        if let Some((user, pass)) = a.split_once(':') {
            cfg.user(user);
            if !pass.is_empty() {
                cfg.password(pass);
            }
        } else if !a.is_empty() {
            cfg.user(a);
        }
    }
    if let Some(q) = query {
        for pair in q.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                match k {
                    "sslmode" => {
                        cfg.ssl_mode(parse_sslmode(v)?);
                    }
                    "connect_timeout" => {
                        if let Ok(secs) = v.parse::<u64>() {
                            cfg.connect_timeout(Duration::from_secs(secs));
                        }
                    }
                    "application_name" => {
                        cfg.application_name(v);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(cfg)
}

fn parse_hostport_path(s: &str) -> Result<(String, u16, Option<String>), String> {
    let (hostport, path) = match s.split_once('/') {
        Some((h, p)) if !p.is_empty() => (h, Some(p.to_string())),
        _ => (s, None),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().map_err(|e: std::num::ParseIntError| e.to_string())?),
        None => (hostport.to_string(), 5432),
    };
    Ok((host, port, path))
}

fn parse_sslmode(s: &str) -> Result<SslMode, String> {
    match s.to_lowercase().as_str() {
        "disable" => Ok(SslMode::Disable),
        "prefer" => Ok(SslMode::Prefer),
        "require" | "verify-ca" | "verify-full" => Ok(SslMode::Require),
        other => Err(format!("unknown sslmode \"{other}\"")),
    }
}
