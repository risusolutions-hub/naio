//! Unix epoll (Linux) / kqueue (macOS) backend.

use super::{Interest, RawSocket};
use std::collections::HashMap;
use std::io;
use std::os::unix::io::AsRawFd;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

pub struct UnixPoller {
    #[cfg(target_os = "linux")]
    inner: linux::EpollPoller,
    #[cfg(target_os = "macos")]
    inner: macos::KqueuePoller,
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    tokens: HashMap<usize, usize>,
}

impl UnixPoller {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            #[cfg(target_os = "linux")]
            inner: linux::EpollPoller::new()?,
            #[cfg(target_os = "macos")]
            inner: macos::KqueuePoller::new()?,
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            tokens: HashMap::new(),
        })
    }

    pub fn register(
        &mut self,
        token: usize,
        raw: RawSocket,
        interest: Interest,
    ) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        return self.inner.register(token, raw, interest);
        #[cfg(target_os = "macos")]
        return self.inner.register(token, raw, interest);
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let fd = raw.as_raw_fd() as usize;
            self.tokens.insert(fd, token);
            Ok(())
        }
    }

    pub fn deregister(&mut self, raw: &RawSocket) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        return self.inner.deregister(raw);
        #[cfg(target_os = "macos")]
        return self.inner.deregister(raw);
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let fd = raw.as_raw_fd() as usize;
            self.tokens.remove(&fd);
            Ok(())
        }
    }

    pub fn poll(&mut self, timeout_ms: Option<u32>) -> io::Result<Vec<usize>> {
        #[cfg(target_os = "linux")]
        return self.inner.poll(timeout_ms);
        #[cfg(target_os = "macos")]
        return self.inner.poll(timeout_ms);
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = timeout_ms;
            std::thread::sleep(std::time::Duration::from_millis(10));
            Ok(self.tokens.values().copied().collect())
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::ptr;

    const EPOLLIN: u32 = 0x001;
    const EPOLLOUT: u32 = 0x004;
    const EPOLL_CTL_ADD: i32 = 1;
    const EPOLL_CTL_DEL: i32 = 2;

    #[repr(C)]
    struct EpollEvent {
        events: u32,
        data: u64,
    }

    #[link(name = "c")]
    extern "C" {
        fn epoll_create1(flags: i32) -> i32;
        fn epoll_ctl(epfd: i32, op: i32, fd: i32, event: *mut EpollEvent) -> i32;
        fn epoll_wait(epfd: i32, events: *mut EpollEvent, maxevents: i32, timeout: i32) -> i32;
    }

    pub struct EpollPoller {
        epfd: i32,
    }

    impl EpollPoller {
        pub fn new() -> io::Result<Self> {
            let epfd = unsafe { epoll_create1(0) };
            if epfd < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self { epfd })
        }

        pub fn register(
            &mut self,
            token: usize,
            raw: RawSocket,
            interest: Interest,
        ) -> io::Result<()> {
            let fd = raw.as_raw_fd();
            let events = match interest {
                Interest::Read => EPOLLIN,
                Interest::Write => EPOLLOUT,
            };
            let mut ev = EpollEvent {
                events,
                data: token as u64,
            };
            let rc = unsafe { epoll_ctl(self.epfd, EPOLL_CTL_ADD, fd, &mut ev) };
            if rc < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        pub fn deregister(&mut self, raw: &RawSocket) -> io::Result<()> {
            let fd = raw.as_raw_fd();
            let rc =
                unsafe { epoll_ctl(self.epfd, EPOLL_CTL_DEL, fd, ptr::null_mut()) };
            if rc < 0 {
                let err = io::Error::last_os_error();
                if err.kind() != io::ErrorKind::NotFound {
                    return Err(err);
                }
            }
            Ok(())
        }

        pub fn poll(&mut self, timeout_ms: Option<u32>) -> io::Result<Vec<usize>> {
            let mut events = [EpollEvent { events: 0, data: 0 }; 64];
            let timeout = timeout_ms.map(|t| t as i32).unwrap_or(-1);
            let n = unsafe {
                epoll_wait(
                    self.epfd,
                    events.as_mut_ptr(),
                    events.len() as i32,
                    timeout,
                )
            };
            if n < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(events[..n as usize]
                .iter()
                .map(|e| e.data as usize)
                .collect())
        }
    }

}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::os::unix::io::AsRawFd;
    use std::ptr;

    const EVFILT_READ: i16 = -1;
    const EVFILT_WRITE: i16 = -2;
    const EV_ADD: u16 = 0x0001;
    const EV_DELETE: u16 = 0x0002;

    #[repr(C)]
    struct Kevent {
        ident: usize,
        filter: i16,
        flags: u16,
        fflags: u32,
        data: isize,
        udata: *mut std::ffi::c_void,
    }

    #[link(name = "c")]
    extern "C" {
        fn kqueue() -> i32;
        fn kevent(
            kq: i32,
            changelist: *const Kevent,
            nchanges: i32,
            eventlist: *mut Kevent,
            nevents: i32,
            timeout: *const libc_timespec,
        ) -> i32;
    }

    #[repr(C)]
    struct libc_timespec {
        tv_sec: i64,
        tv_nsec: i64,
    }

    pub struct KqueuePoller {
        kq: i32,
        token_for_fd: HashMap<i32, usize>,
    }

    impl KqueuePoller {
        pub fn new() -> io::Result<Self> {
            let kq = unsafe { kqueue() };
            if kq < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self {
                kq,
                token_for_fd: HashMap::new(),
            })
        }

        pub fn register(
            &mut self,
            token: usize,
            raw: RawSocket,
            interest: Interest,
        ) -> io::Result<()> {
            let fd = raw.as_raw_fd();
            let filter = match interest {
                Interest::Read => EVFILT_READ,
                Interest::Write => EVFILT_WRITE,
            };
            let ev = Kevent {
                ident: fd as usize,
                filter,
                flags: EV_ADD,
                fflags: 0,
                data: 0,
                udata: ptr::null_mut(),
            };
            let rc = unsafe { kevent(self.kq, &ev, 1, ptr::null_mut(), 0, ptr::null()) };
            if rc < 0 {
                return Err(io::Error::last_os_error());
            }
            self.token_for_fd.insert(fd, token);
            Ok(())
        }

        pub fn deregister(&mut self, raw: &RawSocket) -> io::Result<()> {
            let fd = raw.as_raw_fd();
            self.token_for_fd.remove(&fd);
            for filter in [EVFILT_READ, EVFILT_WRITE] {
                let ev = Kevent {
                    ident: fd as usize,
                    filter,
                    flags: EV_DELETE,
                    fflags: 0,
                    data: 0,
                    udata: ptr::null_mut(),
                };
                unsafe { kevent(self.kq, &ev, 1, ptr::null_mut(), 0, ptr::null()) };
            }
            Ok(())
        }

        pub fn poll(&mut self, timeout_ms: Option<u32>) -> io::Result<Vec<usize>> {
            let mut events = [Kevent {
                ident: 0,
                filter: 0,
                flags: 0,
                fflags: 0,
                data: 0,
                udata: ptr::null_mut(),
            }; 64];
            let ts = timeout_ms.map(|ms| libc_timespec {
                tv_sec: (ms / 1000) as i64,
                tv_nsec: ((ms % 1000) * 1_000_000) as i64,
            });
            let tsp = ts
                .as_ref()
                .map(|t| t as *const libc_timespec)
                .unwrap_or(ptr::null());
            let n = unsafe {
                kevent(
                    self.kq,
                    ptr::null(),
                    0,
                    events.as_mut_ptr(),
                    events.len() as i32,
                    tsp,
                )
            };
            if n < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(events[..n as usize]
                .iter()
                .filter_map(|e| self.token_for_fd.get(&(e.ident as i32)).copied())
                .collect())
        }
    }
}
