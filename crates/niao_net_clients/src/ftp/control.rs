//! FTP control connection (command channel).

use crate::{NetClientError, Result};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Reply {
    pub code: u16,
    pub lines: Vec<String>,
}

pub struct ControlChannel {
    stream: BufReader<TcpStream>,
    scratch: Vec<u8>,
}

impl ControlChannel {
    pub fn connect(addr: &str, _timeout: Duration) -> Result<Self> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true)?;
        Ok(Self {
            stream: BufReader::new(stream),
            scratch: Vec::with_capacity(512),
        })
    }

    pub fn greet(&mut self) -> Result<Reply> {
        self.read_reply()
    }

    #[inline]
    pub fn cmd(&mut self, command: &str) -> Result<Reply> {
        self.scratch.clear();
        self.scratch.extend_from_slice(command.as_bytes());
        self.scratch.extend_from_slice(b"\r\n");
        self.stream.get_mut().write_all(&self.scratch)?;
        self.stream.get_mut().flush()?;
        self.read_reply()
    }

    pub fn read_reply(&mut self) -> Result<Reply> {
        let first = self.read_line()?;
        if first.len() < 3 {
            return Err(NetClientError::Protocol(format!("short reply: {first}")));
        }
        let code = parse_code(&first)?;
        let mut lines = vec![first];
        if lines[0].len() >= 4 && lines[0].as_bytes()[3] == b'-' {
            loop {
                let line = self.read_line()?;
                lines.push(line);
                let last = lines.last().unwrap();
                if last.len() >= 4 && parse_code(last)? == code && last.as_bytes()[3] == b' ' {
                    break;
                }
            }
        }
        Ok(Reply { code, lines })
    }

    fn read_line(&mut self) -> Result<String> {
        let mut line = String::new();
        self.stream
            .read_line(&mut line)
            .map_err(NetClientError::from)?;
        if line.is_empty() {
            return Err(NetClientError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "control connection closed",
            )));
        }
        while line.ends_with('\n') || line.ends_with('\r') {
            line.pop();
        }
        Ok(line)
    }
}

#[inline]
fn parse_code(line: &str) -> Result<u16> {
    let code = line
        .get(..3)
        .ok_or_else(|| NetClientError::Protocol("missing reply code".into()))?
        .parse::<u16>()
        .map_err(|_| NetClientError::Protocol(format!("invalid reply code in: {line}")))?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ftp::mock::MockFtpServer;
    use std::time::Duration;

    #[test]
    fn greet_mock_banner() {
        let server = MockFtpServer::start();
        let addr = format!("127.0.0.1:{}", server.port());
        let mut control = ControlChannel::connect(&addr, Duration::from_secs(30)).unwrap();
        let reply = control.greet().unwrap();
        assert_eq!(reply.code, 220);
        server.shutdown();
    }

    #[test]
    fn raw_user_pass() {
        use std::net::TcpStream;
        let server = MockFtpServer::start();
        let stream = TcpStream::connect(format!("127.0.0.1:{}", server.port())).unwrap();
        let mut io = BufReader::new(stream);
        let mut welcome = String::new();
        io.read_line(&mut welcome).unwrap();
        assert!(welcome.starts_with("220"), "{welcome:?}");
        writeln!(io.get_mut(), "USER user").unwrap();
        io.get_mut().flush().unwrap();
        let mut reply = String::new();
        io.read_line(&mut reply).unwrap();
        assert!(reply.starts_with("331"), "{reply:?}");
        server.shutdown();
    }

    #[test]
    fn login_mock() {
        let server = MockFtpServer::start();
        let addr = format!("127.0.0.1:{}", server.port());
        let mut control = ControlChannel::connect(&addr, Duration::from_secs(30)).unwrap();
        control.greet().unwrap();
        let user = control.cmd("USER user").unwrap();
        assert_eq!(user.code, 331);
        let pass = control.cmd("PASS pass").unwrap();
        assert_eq!(pass.code, 230);
        server.shutdown();
    }

    #[test]
    fn parse_code_ok() {
        assert_eq!(parse_code("220 welcome").unwrap(), 220);
        assert_eq!(parse_code("331 Password required").unwrap(), 331);
    }
}
