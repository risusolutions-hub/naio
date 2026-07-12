//! Windows WSAPoll backend.

use super::{Interest, RawSocket};
use std::collections::HashMap;
use std::io;

#[repr(C)]
struct WSAPollFd {
    socket: usize,
    events: i16,
    revents: i16,
}

const POLLRDNORM: i16 = 0x0100;
const POLLWRNORM: i16 = 0x0010;

#[link(name = "ws2_32")]
extern "system" {
    fn WSAPoll(fds: *mut WSAPollFd, nfds: u32, timeout: i32) -> i32;
}

pub struct WinPoller {
    fds: Vec<WSAPollFd>,
    token_for_socket: HashMap<usize, usize>,
}

impl std::fmt::Debug for WinPoller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WinPoller")
            .field("fds", &self.fds.len())
            .field("tokens", &self.token_for_socket.len())
            .finish()
    }
}

impl WinPoller {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            fds: Vec::new(),
            token_for_socket: HashMap::new(),
        })
    }

    pub fn register(
        &mut self,
        token: usize,
        raw: RawSocket,
        interest: Interest,
    ) -> io::Result<()> {
        let sock = raw.as_raw_socket() as usize;
        self.remove_socket(sock);
        let events = match interest {
            Interest::Read => POLLRDNORM,
            Interest::Write => POLLWRNORM,
        };
        self.fds.push(WSAPollFd {
            socket: sock,
            events,
            revents: 0,
        });
        self.token_for_socket.insert(sock, token);
        Ok(())
    }

    pub fn deregister(&mut self, raw: &RawSocket) -> io::Result<()> {
        let sock = raw.as_raw_socket() as usize;
        self.remove_socket(sock);
        Ok(())
    }

    fn remove_socket(&mut self, sock: usize) {
        self.token_for_socket.remove(&sock);
        self.fds.retain(|f| f.socket != sock);
    }

    pub fn poll(&mut self, timeout_ms: Option<u32>) -> io::Result<Vec<usize>> {
        if self.fds.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(
                timeout_ms.unwrap_or(100) as u64,
            ));
            return Ok(Vec::new());
        }
        for f in &mut self.fds {
            f.revents = 0;
        }
        let timeout = timeout_ms.map(|t| t as i32).unwrap_or(-1);
        let n = unsafe { WSAPoll(self.fds.as_mut_ptr(), self.fds.len() as u32, timeout) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut ready = Vec::new();
        for f in &self.fds {
            if f.revents != 0 {
                if let Some(&token) = self.token_for_socket.get(&f.socket) {
                    ready.push(token);
                }
            }
        }
        Ok(ready)
    }
}

unsafe impl Send for WinPoller {}
