//! Buffered Read/Write over plain TCP or rustls.

use crate::error::{ImapError, Result};
use crate::tls::{wrap_tls, TlsStream};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub enum NetStream {
    Plain(TcpStream),
    Tls(TlsStream),
}

impl Read for NetStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            NetStream::Plain(s) => s.read(buf),
            NetStream::Tls(s) => s.read(buf),
        }
    }
}

impl Write for NetStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            NetStream::Plain(s) => s.write(buf),
            NetStream::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            NetStream::Plain(s) => s.flush(),
            NetStream::Tls(s) => s.flush(),
        }
    }
}

pub struct Conn {
    reader: BufReader<NetStream>,
    timeout: Duration,
}

impl Conn {
    pub fn connect(host: &str, port: u16, timeout: Duration, tls: bool) -> Result<Self> {
        let addr = format!("{host}:{port}");
        let mut last_err = None;
        let mut stream = None;
        for a in addr
            .to_socket_addrs()
            .map_err(|e| ImapError::Io(format!("resolve {addr}: {e}")))?
        {
            match TcpStream::connect_timeout(&a, timeout) {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(e) => last_err = Some(e),
            }
        }
        let stream = stream.ok_or_else(|| {
            ImapError::Io(format!(
                "connect {addr}: {}",
                last_err
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "no addresses".into())
            ))
        })?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let net = if tls {
            NetStream::Tls(wrap_tls(stream, host)?)
        } else {
            NetStream::Plain(stream)
        };
        Ok(Self {
            reader: BufReader::with_capacity(64 * 1024, net),
            timeout,
        })
    }

    pub fn upgrade_tls(self, host: &str) -> Result<Self> {
        let timeout = self.timeout;
        let inner = self.reader.into_inner();
        let stream = match inner {
            NetStream::Plain(s) => s,
            NetStream::Tls(_) => return Err(ImapError::Tls("already using TLS".into())),
        };
        let tls = wrap_tls(stream, host)?;
        Ok(Self {
            reader: BufReader::with_capacity(64 * 1024, NetStream::Tls(tls)),
            timeout,
        })
    }

    pub fn write_all(&mut self, data: &[u8]) -> Result<()> {
        self.reader.get_mut().write_all(data)?;
        self.reader.get_mut().flush()?;
        Ok(())
    }

    pub fn write_line(&mut self, line: &str) -> Result<()> {
        let mut buf = String::with_capacity(line.len() + 2);
        buf.push_str(line);
        buf.push_str("\r\n");
        self.write_all(buf.as_bytes())
    }

    pub fn read_line(&mut self) -> Result<String> {
        let mut line = String::with_capacity(256);
        let n = self.reader.read_line(&mut line)?;
        if n == 0 {
            return Err(ImapError::Io("connection closed".into()));
        }
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        Ok(line)
    }

    pub fn read_exact(&mut self, n: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; n];
        self.reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.timeout = timeout;
        match self.reader.get_mut() {
            NetStream::Plain(s) => {
                s.set_read_timeout(Some(timeout))?;
                s.set_write_timeout(Some(timeout))?;
            }
            NetStream::Tls(s) => {
                s.get_mut().set_read_timeout(Some(timeout))?;
                s.get_mut().set_write_timeout(Some(timeout))?;
            }
        }
        Ok(())
    }
}
