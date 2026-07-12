//! Non-blocking TCP helpers built on the poller.

use crate::{Interest, Poller, RawSocket};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

pub fn tcp_connect(addr: &str, timeout: Duration) -> io::Result<TcpStream> {
    let stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    Ok(stream)
}

pub fn tcp_listen(addr: &str) -> io::Result<TcpListener> {
    let listener = TcpListener::bind(addr)?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

pub fn tcp_accept(listener: &TcpListener) -> io::Result<(TcpStream, SocketAddr)> {
    match listener.accept() {
        Ok(pair) => Ok(pair),
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Err(e),
        Err(e) => Err(e),
    }
}

pub fn tcp_read(stream: &mut TcpStream, buf: &mut [u8]) -> io::Result<usize> {
    stream.read(buf)
}

pub fn tcp_write(stream: &mut TcpStream, buf: &[u8]) -> io::Result<usize> {
    stream.write(buf)
}

/// Poll until `stream` is readable or timeout.
pub fn wait_readable(stream: &TcpStream, timeout: Duration) -> io::Result<bool> {
    let mut poller = Poller::new()?;
    let raw = RawSocket::from(stream.try_clone()?);
    raw.set_nonblocking(true)?;
    poller.register(1, raw, Interest::Read)?;
    let ms = timeout.as_millis().min(u32::MAX as u128) as u32;
    Ok(!poller.poll(Some(ms))?.is_empty())
}

/// Poll until `stream` is writable or timeout.
pub fn wait_writable(stream: &TcpStream, timeout: Duration) -> io::Result<bool> {
    let mut poller = Poller::new()?;
    let raw = RawSocket::from(stream.try_clone()?);
    raw.set_nonblocking(true)?;
    poller.register(1, raw, Interest::Write)?;
    let ms = timeout.as_millis().min(u32::MAX as u128) as u32;
    Ok(!poller.poll(Some(ms))?.is_empty())
}
