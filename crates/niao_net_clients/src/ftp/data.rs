//! FTP data channel helpers — passive/active transfers.

use crate::{NetClientError, Result};
use std::io::{self, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use super::control::ControlChannel;
use super::TransferMode;

#[inline]
pub fn parse_pasv(reply: &str) -> Result<SocketAddr> {
    let start = reply
        .find('(')
        .ok_or_else(|| NetClientError::Protocol("PASV missing '('".into()))?;
    let end = reply
        .find(')')
        .ok_or_else(|| NetClientError::Protocol("PASV missing ')'".into()))?;
    let inner = &reply[start + 1..end];
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() != 6 {
        return Err(NetClientError::Protocol(format!(
            "expected 6 PASV octets, got {}",
            parts.len()
        )));
    }
    let nums: Vec<u8> = parts
        .iter()
        .map(|p| {
            p.parse::<u16>()
                .map(|n| n as u8)
                .map_err(|_| NetClientError::Protocol(format!("invalid PASV octet: {p}")))
        })
        .collect::<Result<Vec<_>>>()?;
    let ip = format!("{}.{}.{}.{}", nums[0], nums[1], nums[2], nums[3]);
    let port = ((nums[4] as u16) << 8) | nums[5] as u16;
    let addr = format!("{ip}:{port}");
    addr.parse()
        .map_err(|e| NetClientError::Protocol(format!("invalid PASV address: {e}")))
}

#[inline]
fn port_command(listener: &TcpListener) -> Result<String> {
    let local = listener.local_addr()?;
    match local.ip() {
        std::net::IpAddr::V4(v4) => {
            let [a, b, c, d] = v4.octets();
            let port = local.port();
            Ok(format!(
                "PORT {a},{b},{c},{d},{},{}",
                port >> 8,
                port & 0xff
            ))
        }
        std::net::IpAddr::V6(_) => Err(NetClientError::Protocol(
            "active mode requires IPv4 control connection".into(),
        )),
    }
}

pub struct DataTransfer<'a> {
    control: &'a mut ControlChannel,
    mode: TransferMode,
    timeout: Duration,
}

impl<'a> DataTransfer<'a> {
    pub fn new(control: &'a mut ControlChannel, mode: TransferMode, timeout: Duration) -> Self {
        Self {
            control,
            mode,
            timeout,
        }
    }

    pub fn download(&mut self, command: &str) -> Result<Vec<u8>> {
        match self.mode {
            TransferMode::Passive => {
                let mut data = self.open_passive()?;
                data.set_read_timeout(Some(self.timeout))?;
                let reply = self.control.cmd(command)?;
                Self::expect_data_start(reply)?;
                let mut out = Vec::new();
                io::copy(&mut data, &mut out)?;
                drop(data);
                let done = self.control.read_reply()?;
                Self::expect_transfer_done(done)?;
                Ok(out)
            }
            TransferMode::Active => {
                let listener = self.prepare_active_listener()?;
                let reply = self.control.cmd(command)?;
                Self::expect_data_start(reply)?;
                let (mut data, _) = self.accept_active(listener)?;
                data.set_read_timeout(Some(self.timeout))?;
                let mut out = Vec::new();
                io::copy(&mut data, &mut out)?;
                drop(data);
                let done = self.control.read_reply()?;
                Self::expect_transfer_done(done)?;
                Ok(out)
            }
        }
    }

    pub fn upload(&mut self, command: &str, payload: &[u8]) -> Result<()> {
        match self.mode {
            TransferMode::Passive => {
                let mut data = self.open_passive()?;
                data.set_write_timeout(Some(self.timeout))?;
                let reply = self.control.cmd(command)?;
                Self::expect_data_start(reply)?;
                data.write_all(payload)?;
                drop(data);
                let done = self.control.read_reply()?;
                Self::expect_transfer_done(done)?;
                Ok(())
            }
            TransferMode::Active => {
                let listener = self.prepare_active_listener()?;
                let reply = self.control.cmd(command)?;
                Self::expect_data_start(reply)?;
                let (mut data, _) = self.accept_active(listener)?;
                data.set_write_timeout(Some(self.timeout))?;
                data.write_all(payload)?;
                drop(data);
                let done = self.control.read_reply()?;
                Self::expect_transfer_done(done)?;
                Ok(())
            }
        }
    }

    fn open_passive(&mut self) -> Result<TcpStream> {
        let reply = self.control.cmd("PASV")?;
        if reply.code != 227 {
            return Err(NetClientError::UnexpectedReply {
                expected: 227,
                got: reply.code,
            });
        }
        let joined = reply.lines.join("");
        let addr = parse_pasv(&joined)?;
        TcpStream::connect(addr).map_err(Into::into)
    }

    fn prepare_active_listener(&mut self) -> Result<TcpListener> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port_cmd = port_command(&listener)?;
        let reply = self.control.cmd(&port_cmd)?;
        if reply.code != 200 {
            return Err(NetClientError::UnexpectedReply {
                expected: 200,
                got: reply.code,
            });
        }
        Ok(listener)
    }

    fn accept_active(&self, listener: TcpListener) -> Result<(TcpStream, SocketAddr)> {
        listener.set_nonblocking(false)?;
        listener.accept().map_err(Into::into)
    }

    fn expect_data_start(reply: super::control::Reply) -> Result<()> {
        if reply.code != 125 && reply.code != 150 {
            return Err(NetClientError::UnexpectedReply {
                expected: 150,
                got: reply.code,
            });
        }
        Ok(())
    }

    fn expect_transfer_done(reply: super::control::Reply) -> Result<()> {
        if reply.code != 226 && reply.code != 250 {
            return Err(NetClientError::UnexpectedReply {
                expected: 226,
                got: reply.code,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasv_parse_ipv4() {
        let addr = parse_pasv("227 Entering Passive Mode (127,0,0,1,195,149).").unwrap();
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
        assert_eq!(addr.port(), 195 * 256 + 149);
    }
}
