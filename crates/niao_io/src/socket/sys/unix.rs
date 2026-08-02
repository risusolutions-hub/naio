//! Unix socket syscalls.

use std::io;
use std::mem;
use std::net::SocketAddr;
use std::os::raw::{c_int, c_void};
use std::os::unix::io::RawFd;
use std::time::Duration;

pub const AF_INET: c_int = 2;
pub const AF_INET6: c_int = 10;
pub const SOCK_STREAM: c_int = 1;
pub const SOCK_DGRAM: c_int = 2;
pub const IPPROTO_TCP: c_int = 6;
pub const SOL_SOCKET: c_int = 1;
pub const SO_REUSEADDR: c_int = 2;
pub const SO_KEEPALIVE: c_int = 9;
pub const SO_RCVTIMEO: c_int = 20;
pub const SO_SNDTIMEO: c_int = 21;
pub const TCP_NODELAY: c_int = 1;

#[repr(C)]
struct SockAddrIn {
    sin_family: u16,
    sin_port: u16,
    sin_addr: [u8; 4],
    sin_zero: [u8; 8],
}

#[repr(C)]
struct SockAddrIn6 {
    sin6_family: u16,
    sin6_port: u16,
    sin6_flowinfo: u32,
    sin6_addr: [u8; 16],
    sin6_scope_id: u32,
}

#[repr(C)]
struct TimeVal {
    tv_sec: i64,
    tv_usec: i64,
}

#[link(name = "c")]
extern "C" {
    fn socket(domain: c_int, typ: c_int, protocol: c_int) -> c_int;
    fn bind(sockfd: c_int, addr: *const c_void, addrlen: u32) -> c_int;
    fn connect(sockfd: c_int, addr: *const c_void, addrlen: u32) -> c_int;
    fn listen(sockfd: c_int, backlog: c_int) -> c_int;
    fn setsockopt(
        sockfd: c_int,
        level: c_int,
        optname: c_int,
        optval: *const c_void,
        optlen: u32,
    ) -> c_int;
    fn getsockopt(
        sockfd: c_int,
        level: c_int,
        optname: c_int,
        optval: *mut c_void,
        optlen: *mut u32,
    ) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn fcntl(fd: c_int, cmd: c_int, arg: c_int) -> c_int;
}

const F_GETFL: c_int = 3;
const F_SETFL: c_int = 4;
const O_NONBLOCK: c_int = 0x800;

#[inline]
fn last_err() -> io::Error {
    io::Error::last_os_error()
}

pub fn create_socket(domain: c_int, typ: c_int, protocol: c_int) -> io::Result<RawFd> {
    let fd = unsafe { socket(domain, typ, protocol) };
    if fd < 0 {
        return Err(last_err());
    }
    Ok(fd)
}

pub fn close_socket(fd: RawFd) {
    unsafe {
        let _ = close(fd);
    }
}

pub fn set_nonblocking(fd: RawFd, nonblocking: bool) -> io::Result<()> {
    let flags = unsafe { fcntl(fd, F_GETFL, 0) };
    if flags < 0 {
        return Err(last_err());
    }
    let new_flags = if nonblocking {
        flags | O_NONBLOCK
    } else {
        flags & !O_NONBLOCK
    };
    if unsafe { fcntl(fd, F_SETFL, new_flags) } < 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn encode_addr(addr: &SocketAddr) -> (Vec<u8>, u32) {
    match addr {
        SocketAddr::V4(v4) => {
            let sa = SockAddrIn {
                sin_family: AF_INET as u16,
                sin_port: v4.port().to_be(),
                sin_addr: v4.ip().octets(),
                sin_zero: [0; 8],
            };
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    &sa as *const _ as *const u8,
                    mem::size_of::<SockAddrIn>(),
                )
            }
            .to_vec();
            (bytes, mem::size_of::<SockAddrIn>() as u32)
        }
        SocketAddr::V6(v6) => {
            let sa = SockAddrIn6 {
                sin6_family: AF_INET6 as u16,
                sin6_port: v6.port().to_be(),
                sin6_flowinfo: v6.flowinfo(),
                sin6_addr: v6.ip().octets(),
                sin6_scope_id: v6.scope_id(),
            };
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    &sa as *const _ as *const u8,
                    mem::size_of::<SockAddrIn6>(),
                )
            }
            .to_vec();
            (bytes, mem::size_of::<SockAddrIn6>() as u32)
        }
    }
}

pub fn bind_socket(fd: RawFd, addr: &SocketAddr) -> io::Result<()> {
    let (bytes, len) = encode_addr(addr);
    let rc = unsafe { bind(fd, bytes.as_ptr() as *const c_void, len) };
    if rc < 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn connect_socket(fd: RawFd, addr: &SocketAddr) -> io::Result<()> {
    let (bytes, len) = encode_addr(addr);
    let rc = unsafe { connect(fd, bytes.as_ptr() as *const c_void, len) };
    if rc < 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn listen_socket(fd: RawFd, backlog: i32) -> io::Result<()> {
    let rc = unsafe { listen(fd, backlog) };
    if rc < 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn set_reuse_addr(fd: RawFd, on: bool) -> io::Result<()> {
    let val: c_int = i32::from(on);
    let rc = unsafe {
        setsockopt(
            fd,
            SOL_SOCKET,
            SO_REUSEADDR,
            &val as *const _ as *const c_void,
            mem::size_of::<c_int>() as u32,
        )
    };
    if rc < 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn get_reuse_addr(fd: RawFd) -> io::Result<bool> {
    let mut val: c_int = 0;
    let mut len = mem::size_of::<c_int>() as u32;
    let rc = unsafe {
        getsockopt(
            fd,
            SOL_SOCKET,
            SO_REUSEADDR,
            &mut val as *mut _ as *mut c_void,
            &mut len,
        )
    };
    if rc < 0 {
        return Err(last_err());
    }
    Ok(val != 0)
}

pub fn set_nodelay(fd: RawFd, on: bool) -> io::Result<()> {
    let val: c_int = i32::from(on);
    let rc = unsafe {
        setsockopt(
            fd,
            IPPROTO_TCP,
            TCP_NODELAY,
            &val as *const _ as *const c_void,
            mem::size_of::<c_int>() as u32,
        )
    };
    if rc < 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn get_nodelay(fd: RawFd) -> io::Result<bool> {
    let mut val: c_int = 0;
    let mut len = mem::size_of::<c_int>() as u32;
    let rc = unsafe {
        getsockopt(
            fd,
            IPPROTO_TCP,
            TCP_NODELAY,
            &mut val as *mut _ as *mut c_void,
            &mut len,
        )
    };
    if rc < 0 {
        return Err(last_err());
    }
    Ok(val != 0)
}

pub fn set_keepalive(fd: RawFd, on: bool) -> io::Result<()> {
    let val: c_int = i32::from(on);
    let rc = unsafe {
        setsockopt(
            fd,
            SOL_SOCKET,
            SO_KEEPALIVE,
            &val as *const _ as *const c_void,
            mem::size_of::<c_int>() as u32,
        )
    };
    if rc < 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn get_keepalive(fd: RawFd) -> io::Result<bool> {
    let mut val: c_int = 0;
    let mut len = mem::size_of::<c_int>() as u32;
    let rc = unsafe {
        getsockopt(
            fd,
            SOL_SOCKET,
            SO_KEEPALIVE,
            &mut val as *mut _ as *mut c_void,
            &mut len,
        )
    };
    if rc < 0 {
        return Err(last_err());
    }
    Ok(val != 0)
}

fn duration_to_timeval(d: Duration) -> TimeVal {
    TimeVal {
        tv_sec: d.as_secs() as i64,
        tv_usec: d.subsec_micros() as i64,
    }
}

fn timeval_to_duration(tv: TimeVal) -> Option<Duration> {
    if tv.tv_sec == 0 && tv.tv_usec == 0 {
        None
    } else {
        Some(Duration::new(tv.tv_sec as u64, (tv.tv_usec * 1000) as u32))
    }
}

pub fn set_read_timeout(fd: RawFd, timeout: Option<Duration>) -> io::Result<()> {
    let tv = match timeout {
        None => TimeVal {
            tv_sec: 0,
            tv_usec: 0,
        },
        Some(d) => duration_to_timeval(d),
    };
    let rc = unsafe {
        setsockopt(
            fd,
            SOL_SOCKET,
            SO_RCVTIMEO,
            &tv as *const _ as *const c_void,
            mem::size_of::<TimeVal>() as u32,
        )
    };
    if rc < 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn get_read_timeout(fd: RawFd) -> io::Result<Option<Duration>> {
    let mut tv = TimeVal {
        tv_sec: 0,
        tv_usec: 0,
    };
    let mut len = mem::size_of::<TimeVal>() as u32;
    let rc = unsafe {
        getsockopt(
            fd,
            SOL_SOCKET,
            SO_RCVTIMEO,
            &mut tv as *mut _ as *mut c_void,
            &mut len,
        )
    };
    if rc < 0 {
        return Err(last_err());
    }
    Ok(timeval_to_duration(tv))
}

pub fn set_write_timeout(fd: RawFd, timeout: Option<Duration>) -> io::Result<()> {
    let tv = match timeout {
        None => TimeVal {
            tv_sec: 0,
            tv_usec: 0,
        },
        Some(d) => duration_to_timeval(d),
    };
    let rc = unsafe {
        setsockopt(
            fd,
            SOL_SOCKET,
            SO_SNDTIMEO,
            &tv as *const _ as *const c_void,
            mem::size_of::<TimeVal>() as u32,
        )
    };
    if rc < 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn get_write_timeout(fd: RawFd) -> io::Result<Option<Duration>> {
    let mut tv = TimeVal {
        tv_sec: 0,
        tv_usec: 0,
    };
    let mut len = mem::size_of::<TimeVal>() as u32;
    let rc = unsafe {
        getsockopt(
            fd,
            SOL_SOCKET,
            SO_SNDTIMEO,
            &mut tv as *mut _ as *mut c_void,
            &mut len,
        )
    };
    if rc < 0 {
        return Err(last_err());
    }
    Ok(timeval_to_duration(tv))
}
