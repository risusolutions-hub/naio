//! High-level FTP client.

use crate::{NetClientError, Result};
use std::time::Duration;

use super::control::ControlChannel;
use super::data::DataTransfer;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferMode {
    Passive,
    Active,
}

#[derive(Debug, Clone)]
pub struct FtpOptions {
    pub timeout: Duration,
    pub mode: TransferMode,
    pub use_tls: bool,
}

impl Default for FtpOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            mode: TransferMode::Passive,
            use_tls: false,
        }
    }
}

pub struct FtpClient {
    control: ControlChannel,
    options: FtpOptions,
    binary: bool,
    logged_in: bool,
}

/// Connect to an FTP server (plain FTP control channel).
pub fn connect(host: &str, port: u16) -> Result<FtpClient> {
    connect_with(host, port, FtpOptions::default())
}

/// Connect with explicit options (passive/active, timeout, optional TLS request).
pub fn connect_with(host: &str, port: u16, options: FtpOptions) -> Result<FtpClient> {
    if options.use_tls {
        return Err(NetClientError::TlsUnsupported);
    }
    let addr = format!("{host}:{port}");
    let mut control = ControlChannel::connect(&addr, options.timeout)?;
    control.greet()?;
    Ok(FtpClient {
        control,
        options,
        binary: false,
        logged_in: false,
    })
}

impl FtpClient {
    pub fn login(&mut self, user: &str, pass: &str) -> Result<()> {
        let user_reply = self.control.cmd(&format!("USER {user}"))?;
        match user_reply.code {
            230 => {
                self.logged_in = true;
                return Ok(());
            }
            331 => {}
            code => {
                return Err(NetClientError::UnexpectedReply {
                    expected: 331,
                    got: code,
                });
            }
        }
        let pass_reply = self.control.cmd(&format!("PASS {pass}"))?;
        if pass_reply.code != 230 {
            return Err(NetClientError::UnexpectedReply {
                expected: 230,
                got: pass_reply.code,
            });
        }
        self.logged_in = true;
        Ok(())
    }

    pub fn get(&mut self, remote: &str) -> Result<Vec<u8>> {
        self.ensure_binary()?;
        let mut xfer = DataTransfer::new(&mut self.control, self.options.mode, self.options.timeout);
        xfer.download(&format!("RETR {remote}"))
    }

    pub fn put(&mut self, remote: &str, data: &[u8]) -> Result<()> {
        self.ensure_binary()?;
        let mut xfer = DataTransfer::new(&mut self.control, self.options.mode, self.options.timeout);
        xfer.upload(&format!("STOR {remote}"), data)
    }

    pub fn list(&mut self, path: Option<&str>) -> Result<Vec<String>> {
        self.ensure_binary()?;
        let cmd = match path {
            Some(p) => format!("LIST {p}"),
            None => "LIST".to_string(),
        };
        let mut xfer = DataTransfer::new(&mut self.control, self.options.mode, self.options.timeout);
        let raw = xfer.download(&cmd)?;
        let text = String::from_utf8_lossy(&raw);
        Ok(text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect())
    }

    pub fn quit(&mut self) -> Result<()> {
        let reply = self.control.cmd("QUIT")?;
        if reply.code != 221 {
            return Err(NetClientError::UnexpectedReply {
                expected: 221,
                got: reply.code,
            });
        }
        Ok(())
    }

    pub fn set_mode(&mut self, mode: TransferMode) {
        self.options.mode = mode;
    }

    pub fn is_logged_in(&self) -> bool {
        self.logged_in
    }

    fn ensure_binary(&mut self) -> Result<()> {
        if self.binary {
            return Ok(());
        }
        let reply = self.control.cmd("TYPE I")?;
        if reply.code != 200 {
            return Err(NetClientError::UnexpectedReply {
                expected: 200,
                got: reply.code,
            });
        }
        self.binary = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ftp::mock::MockFtpServer;
    use std::io::Read;

    #[test]
    fn mock_sends_banner() {
        let server = MockFtpServer::start();
        let mut s =
            std::net::TcpStream::connect(format!("127.0.0.1:{}", server.port())).unwrap();
        let mut buf = [0u8; 128];
        let n = s.read(&mut buf).unwrap();
        let banner = String::from_utf8_lossy(&buf[..n]);
        assert!(banner.starts_with("220"), "banner was: {banner:?}");
        server.shutdown();
    }

    #[test]
    fn roundtrip_passive() {
        let server = MockFtpServer::start();
        let port = server.port();
        let mut client = connect("127.0.0.1", port).unwrap();
        client.login("user", "pass").unwrap();
        client.put("hello.txt", b"niao ftp").unwrap();
        let data = client.get("hello.txt").unwrap();
        assert_eq!(data, b"niao ftp");
        let names = client.list(None).unwrap();
        assert!(names.iter().any(|l| l.contains("hello.txt")));
        client.quit().unwrap();
        server.shutdown();
    }

    #[test]
    fn roundtrip_active() {
        let server = MockFtpServer::start();
        let port = server.port();
        let mut client = connect_with(
            "127.0.0.1",
            port,
            FtpOptions {
                mode: TransferMode::Active,
                ..Default::default()
            },
        )
        .unwrap();
        client.login("anon", "anon").unwrap();
        client.put("active.bin", &[1, 2, 3, 4]).unwrap();
        let data = client.get("active.bin").unwrap();
        assert_eq!(data, vec![1, 2, 3, 4]);
        client.quit().unwrap();
        server.shutdown();
    }

    #[test]
    fn tls_option_rejected() {
        assert!(matches!(
            connect_with(
                "127.0.0.1",
                21,
                FtpOptions {
                    use_tls: true,
                    ..Default::default()
                },
            ),
            Err(NetClientError::TlsUnsupported)
        ));
    }
}
