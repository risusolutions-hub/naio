//! Cross-platform readiness poller (WSAPoll / epoll / kqueue).

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::net::TcpStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interest {
    Read,
    Write,
}

#[derive(Debug)]
pub struct Poller {
    #[cfg(windows)]
    inner: windows::WinPoller,
    #[cfg(unix)]
    inner: unix::UnixPoller,
}

impl Poller {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            #[cfg(windows)]
            inner: windows::WinPoller::new()?,
            #[cfg(unix)]
            inner: unix::UnixPoller::new()?,
        })
    }

    pub fn register(
        &mut self,
        token: usize,
        raw: RawSocket,
        interest: Interest,
    ) -> std::io::Result<()> {
        #[cfg(windows)]
        return self.inner.register(token, raw, interest);
        #[cfg(unix)]
        return self.inner.register(token, raw, interest);
    }

    pub fn deregister(&mut self, raw: &RawSocket) -> std::io::Result<()> {
        #[cfg(windows)]
        return self.inner.deregister(raw);
        #[cfg(unix)]
        return self.inner.deregister(raw);
    }

    /// Wait for events. Returns ready token ids. `timeout_ms` None = block forever.
    pub fn poll(&mut self, timeout_ms: Option<u32>) -> std::io::Result<Vec<usize>> {
        #[cfg(windows)]
        return self.inner.poll(timeout_ms);
        #[cfg(unix)]
        return self.inner.poll(timeout_ms);
    }
}

/// Platform socket handle (non-blocking `TcpStream`).
pub struct RawSocket {
    stream: TcpStream,
}

impl From<TcpStream> for RawSocket {
    fn from(stream: TcpStream) -> Self {
        Self { stream }
    }
}

impl RawSocket {
    pub fn stream(&self) -> &TcpStream {
        &self.stream
    }

    pub fn into_stream(self) -> TcpStream {
        self.stream
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> std::io::Result<()> {
        self.stream.set_nonblocking(nonblocking)
    }
}

#[cfg(windows)]
impl RawSocket {
    pub fn as_raw_socket(&self) -> std::os::windows::io::RawSocket {
        use std::os::windows::io::AsRawSocket;
        self.stream.as_raw_socket()
    }
}

#[cfg(unix)]
impl RawSocket {
    pub fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        use std::os::unix::io::AsRawFd;
        self.stream.as_raw_fd()
    }
}
