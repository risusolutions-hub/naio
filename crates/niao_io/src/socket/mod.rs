//! Thin safe wrapper over raw socket()/setsockopt/bind/connect.
//!
//! Replaces the `socket2` crate for Niao — zero external dependencies.

mod sys;

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::os::raw::c_int;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{FromRawSocket, RawSocket};

/// Communication domain for a socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Domain {
    Ipv4,
    Ipv6,
}

impl Domain {
    #[inline]
    fn as_raw(self) -> c_int {
        match self {
            Self::Ipv4 => sys::AF_INET,
            Self::Ipv6 => sys::AF_INET6,
        }
    }
}

/// Socket type (stream / datagram).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    Stream,
    Dgram,
}

impl Type {
    #[inline]
    fn as_raw(self) -> c_int {
        match self {
            Self::Stream => sys::SOCK_STREAM,
            Self::Dgram => sys::SOCK_DGRAM,
        }
    }
}

/// Optional protocol (typically `None` for default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    Tcp,
    Udp,
}

impl Protocol {
    #[inline]
    fn as_raw(self) -> c_int {
        match self {
            Self::Tcp => sys::IPPROTO_TCP,
            Self::Udp => 17, // IPPROTO_UDP
        }
    }
}

/// Which socket option to read via [`Socket::get_opt`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SocketOptionKind {
    ReuseAddress,
    Nodelay,
    Keepalive,
    ReadTimeout,
    WriteTimeout,
}

/// Socket option value for [`Socket::set_opt`] / [`Socket::get_opt`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketOption {
    ReuseAddress(bool),
    Nodelay(bool),
    Keepalive(bool),
    ReadTimeout(Option<Duration>),
    WriteTimeout(Option<Duration>),
}

impl SocketOption {
    #[inline]
    pub fn kind(&self) -> SocketOptionKind {
        match self {
            Self::ReuseAddress(_) => SocketOptionKind::ReuseAddress,
            Self::Nodelay(_) => SocketOptionKind::Nodelay,
            Self::Keepalive(_) => SocketOptionKind::Keepalive,
            Self::ReadTimeout(_) => SocketOptionKind::ReadTimeout,
            Self::WriteTimeout(_) => SocketOptionKind::WriteTimeout,
        }
    }
}

/// Socket address wrapper (mirrors `socket2::SockAddr` for IP endpoints).
#[derive(Debug, Clone)]
pub struct SockAddr {
    addr: SocketAddr,
}

impl SockAddr {
    #[inline]
    pub fn as_socket_addr(&self) -> &SocketAddr {
        &self.addr
    }
}

impl From<SocketAddr> for SockAddr {
    #[inline]
    fn from(addr: SocketAddr) -> Self {
        Self { addr }
    }
}

impl From<&SocketAddr> for SockAddr {
    #[inline]
    fn from(addr: &SocketAddr) -> Self {
        Self { addr: *addr }
    }
}

/// Owned wrapper around a system socket handle.
pub struct Socket {
    #[cfg(unix)]
    fd: RawFd,
    #[cfg(windows)]
    handle: RawSocket,
}

impl Socket {
    /// Create a new socket (`socket(2)` / `WSASocket`).
    #[inline]
    pub fn new(domain: Domain, ty: Type, protocol: Option<Protocol>) -> io::Result<Self> {
        let proto = protocol.map(Protocol::as_raw).unwrap_or(0);
        let fd = sys::create_socket(domain.as_raw(), ty.as_raw(), proto)?;
        Ok(Self {
            #[cfg(unix)]
            fd,
            #[cfg(windows)]
            handle: fd,
        })
    }

    #[inline]
    #[cfg(unix)]
    fn raw(&self) -> RawFd {
        self.fd
    }

    #[inline]
    #[cfg(windows)]
    fn raw(&self) -> RawSocket {
        self.handle
    }

    /// Bind to `address` (`bind(2)`).
    #[inline]
    pub fn bind(&self, address: &SockAddr) -> io::Result<()> {
        sys::bind_socket(self.raw(), &address.addr)
    }

    /// Connect to `address` (`connect(2)`).
    #[inline]
    pub fn connect(&self, address: &SockAddr) -> io::Result<()> {
        sys::connect_socket(self.raw(), &address.addr)
    }

    /// Listen for incoming connections (`listen(2)`).
    #[inline]
    pub fn listen(&self, backlog: i32) -> io::Result<()> {
        sys::listen_socket(self.raw(), backlog)
    }

    /// Set a socket option.
    #[inline]
    pub fn set_opt(&self, opt: &SocketOption) -> io::Result<()> {
        match opt {
            SocketOption::ReuseAddress(v) => sys::set_reuse_addr(self.raw(), *v),
            SocketOption::Nodelay(v) => sys::set_nodelay(self.raw(), *v),
            SocketOption::Keepalive(v) => sys::set_keepalive(self.raw(), *v),
            SocketOption::ReadTimeout(v) => sys::set_read_timeout(self.raw(), *v),
            SocketOption::WriteTimeout(v) => sys::set_write_timeout(self.raw(), *v),
        }
    }

    /// Read a socket option (round-trip with [`Socket::set_opt`]).
    #[inline]
    pub fn get_opt(&self, kind: SocketOptionKind) -> io::Result<SocketOption> {
        let opt = match kind {
            SocketOptionKind::ReuseAddress => {
                SocketOption::ReuseAddress(sys::get_reuse_addr(self.raw())?)
            }
            SocketOptionKind::Nodelay => SocketOption::Nodelay(sys::get_nodelay(self.raw())?),
            SocketOptionKind::Keepalive => SocketOption::Keepalive(sys::get_keepalive(self.raw())?),
            SocketOptionKind::ReadTimeout => {
                SocketOption::ReadTimeout(sys::get_read_timeout(self.raw())?)
            }
            SocketOptionKind::WriteTimeout => {
                SocketOption::WriteTimeout(sys::get_write_timeout(self.raw())?)
            }
        };
        Ok(opt)
    }

    /// Set non-blocking mode.
    #[inline]
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        sys::set_nonblocking(self.raw(), nonblocking)
    }

    /// Convenience: `set_opt(ReuseAddress(on))`.
    #[inline]
    pub fn set_reuse_address(&self, on: bool) -> io::Result<()> {
        self.set_opt(&SocketOption::ReuseAddress(on))
    }

    /// Convenience: `set_opt(Nodelay(on))`.
    #[inline]
    pub fn set_nodelay(&self, on: bool) -> io::Result<()> {
        self.set_opt(&SocketOption::Nodelay(on))
    }

    /// Convenience: `set_opt(Keepalive(on))`.
    #[inline]
    pub fn set_keepalive(&self, on: bool) -> io::Result<()> {
        self.set_opt(&SocketOption::Keepalive(on))
    }

    /// Take ownership of the underlying handle without closing it.
    #[inline]
    #[cfg(unix)]
    fn into_raw(self) -> RawFd {
        let fd = self.fd;
        std::mem::forget(self);
        fd
    }

    #[inline]
    #[cfg(windows)]
    fn into_raw(self) -> RawSocket {
        let handle = self.handle;
        std::mem::forget(self);
        handle
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        sys::close_socket(self.raw());
    }
}

#[cfg(unix)]
impl From<Socket> for TcpStream {
    fn from(socket: Socket) -> Self {
        let fd = socket.into_raw();
        unsafe { TcpStream::from_raw_fd(fd) }
    }
}

#[cfg(unix)]
impl From<Socket> for TcpListener {
    fn from(socket: Socket) -> Self {
        let fd = socket.into_raw();
        unsafe { TcpListener::from_raw_fd(fd) }
    }
}

#[cfg(unix)]
impl From<Socket> for UdpSocket {
    fn from(socket: Socket) -> Self {
        let fd = socket.into_raw();
        unsafe { UdpSocket::from_raw_fd(fd) }
    }
}

#[cfg(windows)]
impl From<Socket> for TcpStream {
    fn from(socket: Socket) -> Self {
        let handle = socket.into_raw();
        unsafe { TcpStream::from_raw_socket(handle) }
    }
}

#[cfg(windows)]
impl From<Socket> for TcpListener {
    fn from(socket: Socket) -> Self {
        let handle = socket.into_raw();
        unsafe { TcpListener::from_raw_socket(handle) }
    }
}

#[cfg(windows)]
impl From<Socket> for UdpSocket {
    fn from(socket: Socket) -> Self {
        let handle = socket.into_raw();
        unsafe { UdpSocket::from_raw_socket(handle) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tcp_socket() -> Socket {
        Socket::new(Domain::Ipv4, Type::Stream, None).expect("socket")
    }

    fn round_trip(opt: SocketOption) {
        let sock = tcp_socket();
        let kind = opt.kind();
        sock.set_opt(&opt).expect("set_opt");
        let got = sock.get_opt(kind).expect("get_opt");
        assert_eq!(got, opt, "round-trip mismatch for {kind:?}");
    }

    #[test]
    fn option_reuse_address_roundtrip() {
        round_trip(SocketOption::ReuseAddress(true));
        round_trip(SocketOption::ReuseAddress(false));
    }

    #[test]
    fn option_nodelay_roundtrip() {
        round_trip(SocketOption::Nodelay(true));
        round_trip(SocketOption::Nodelay(false));
    }

    #[test]
    fn option_keepalive_roundtrip() {
        round_trip(SocketOption::Keepalive(true));
        round_trip(SocketOption::Keepalive(false));
    }

    #[test]
    fn option_read_timeout_roundtrip() {
        round_trip(SocketOption::ReadTimeout(None));
        round_trip(SocketOption::ReadTimeout(Some(Duration::from_millis(500))));
    }

    #[test]
    fn option_write_timeout_roundtrip() {
        round_trip(SocketOption::WriteTimeout(None));
        round_trip(SocketOption::WriteTimeout(Some(Duration::from_millis(750))));
    }

    #[test]
    fn bind_connect_loopback() {
        let listener_sock = tcp_socket();
        listener_sock.set_reuse_address(true).expect("reuseaddr");
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        listener_sock.bind(&addr.into()).expect("bind");
        listener_sock.listen(8).expect("listen");
        let listener: TcpListener = listener_sock.into();
        let bound = listener.local_addr().unwrap();

        let client = tcp_socket();
        client.connect(&bound.into()).expect("connect");
        let _stream: TcpStream = client.into();

        let (accepted, _) = listener.accept().expect("accept");
        drop(accepted);
    }

    #[test]
    fn socket_new_bind_ephemeral() {
        let sock = tcp_socket();
        sock.bind(&"0.0.0.0:0".parse::<SocketAddr>().unwrap().into())
            .expect("bind");
        let stream: TcpStream = sock.into();
        let addr = stream.local_addr().expect("local_addr");
        assert!(addr.port() > 0);
    }

    #[test]
    fn reuseaddr_allows_double_bind() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let a = tcp_socket();
        a.set_reuse_address(true).unwrap();
        a.bind(&addr.into()).unwrap();
        a.listen(4).unwrap();
        let bound = {
            let l: TcpListener = a.into();
            l.local_addr().unwrap()
        };

        let b = tcp_socket();
        b.set_reuse_address(true).unwrap();
        b.bind(&bound.into())
            .expect("second bind with SO_REUSEADDR");
    }
}
