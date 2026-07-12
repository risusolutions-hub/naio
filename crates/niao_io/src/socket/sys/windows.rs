//! WinSock socket syscalls.

use std::io;
use std::mem;
use std::net::SocketAddr;
use std::os::raw::{c_char, c_int, c_ulong, c_void};
use std::os::windows::io::RawSocket;
use std::time::Duration;

pub const AF_INET: c_int = 2;
pub const AF_INET6: c_int = 23;
pub const SOCK_STREAM: c_int = 1;
pub const SOCK_DGRAM: c_int = 2;
pub const IPPROTO_TCP: c_int = 6;
pub const SOL_SOCKET: c_int = 0xffff;
pub const SO_REUSEADDR: c_int = 4;
pub const SO_KEEPALIVE: c_int = 8;
pub const SO_RCVTIMEO: c_int = 0x1006;
pub const SO_SNDTIMEO: c_int = 0x1005;
pub const TCP_NODELAY: c_int = 1;
pub const FIONBIO: c_ulong = 0x8004667e;
pub const INVALID_SOCKET: RawSocket = !0;

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
struct WSAData {
    version: u16,
    high_version: u16,
    max_sockets: u16,
    max_udp_dg: u16,
    vendor_info: *mut c_char,
    description: *mut c_char,
    system_status: *mut c_char,
}

#[link(name = "ws2_32")]
extern "system" {
    fn WSAStartup(version: u16, data: *mut WSAData) -> c_int;
    fn socket(af: c_int, typ: c_int, protocol: c_int) -> RawSocket;
    fn bind(s: RawSocket, addr: *const c_void, namelen: c_int) -> c_int;
    fn connect(s: RawSocket, addr: *const c_void, namelen: c_int) -> c_int;
    fn listen(s: RawSocket, backlog: c_int) -> c_int;
    fn setsockopt(
        s: RawSocket,
        level: c_int,
        optname: c_int,
        optval: *const c_char,
        optlen: c_int,
    ) -> c_int;
    fn getsockopt(
        s: RawSocket,
        level: c_int,
        optname: c_int,
        optval: *mut c_char,
        optlen: *mut c_int,
    ) -> c_int;
    fn closesocket(s: RawSocket) -> c_int;
    fn ioctlsocket(s: RawSocket, cmd: c_ulong, argp: *mut u32) -> c_int;
}

static INIT: std::sync::Once = std::sync::Once::new();

fn ensure_wsa() {
    INIT.call_once(|| {
        let mut data: WSAData = unsafe { mem::zeroed() };
        let rc = unsafe { WSAStartup(0x0202, &mut data) };
        if rc != 0 {
            panic!("WSAStartup failed with code {rc}");
        }
    });
}

#[inline]
fn last_err() -> io::Error {
    io::Error::last_os_error()
}

pub fn create_socket(domain: c_int, typ: c_int, protocol: c_int) -> io::Result<RawSocket> {
    ensure_wsa();
    let s = unsafe { socket(domain, typ, protocol) };
    if s == INVALID_SOCKET {
        return Err(last_err());
    }
    Ok(s)
}

pub fn close_socket(s: RawSocket) {
    unsafe {
        let _ = closesocket(s);
    }
}

pub fn set_nonblocking(s: RawSocket, nonblocking: bool) -> io::Result<()> {
    let mut mode: u32 = u32::from(nonblocking);
    if unsafe { ioctlsocket(s, FIONBIO, &mut mode) } != 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn encode_addr(addr: &SocketAddr) -> (Vec<u8>, c_int) {
    match addr {
        SocketAddr::V4(v4) => {
            let sa = SockAddrIn {
                sin_family: AF_INET as u16,
                sin_port: v4.port().to_be(),
                sin_addr: v4.ip().octets(),
                sin_zero: [0; 8],
            };
            let bytes =
                unsafe { std::slice::from_raw_parts(&sa as *const _ as *const u8, mem::size_of::<SockAddrIn>()) }
                    .to_vec();
            (bytes, mem::size_of::<SockAddrIn>() as c_int)
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
                std::slice::from_raw_parts(&sa as *const _ as *const u8, mem::size_of::<SockAddrIn6>())
            }
            .to_vec();
            (bytes, mem::size_of::<SockAddrIn6>() as c_int)
        }
    }
}

pub fn bind_socket(s: RawSocket, addr: &SocketAddr) -> io::Result<()> {
    let (bytes, len) = encode_addr(addr);
    let rc = unsafe { bind(s, bytes.as_ptr() as *const c_void, len) };
    if rc != 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn connect_socket(s: RawSocket, addr: &SocketAddr) -> io::Result<()> {
    let (bytes, len) = encode_addr(addr);
    let rc = unsafe { connect(s, bytes.as_ptr() as *const c_void, len) };
    if rc != 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn listen_socket(s: RawSocket, backlog: i32) -> io::Result<()> {
    let rc = unsafe { listen(s, backlog) };
    if rc != 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn set_reuse_addr(s: RawSocket, on: bool) -> io::Result<()> {
    let val: c_int = i32::from(on);
    let rc = unsafe {
        setsockopt(
            s,
            SOL_SOCKET,
            SO_REUSEADDR,
            &val as *const _ as *const c_char,
            mem::size_of::<c_int>() as c_int,
        )
    };
    if rc != 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn get_reuse_addr(s: RawSocket) -> io::Result<bool> {
    let mut val: c_int = 0;
    let mut len = mem::size_of::<c_int>() as c_int;
    let rc = unsafe {
        getsockopt(
            s,
            SOL_SOCKET,
            SO_REUSEADDR,
            &mut val as *mut _ as *mut c_char,
            &mut len,
        )
    };
    if rc != 0 {
        return Err(last_err());
    }
    Ok(val != 0)
}

pub fn set_nodelay(s: RawSocket, on: bool) -> io::Result<()> {
    let val: c_int = i32::from(on);
    let rc = unsafe {
        setsockopt(
            s,
            IPPROTO_TCP,
            TCP_NODELAY,
            &val as *const _ as *const c_char,
            mem::size_of::<c_int>() as c_int,
        )
    };
    if rc != 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn get_nodelay(s: RawSocket) -> io::Result<bool> {
    let mut val: c_int = 0;
    let mut len = mem::size_of::<c_int>() as c_int;
    let rc = unsafe {
        getsockopt(
            s,
            IPPROTO_TCP,
            TCP_NODELAY,
            &mut val as *mut _ as *mut c_char,
            &mut len,
        )
    };
    if rc != 0 {
        return Err(last_err());
    }
    Ok(val != 0)
}

pub fn set_keepalive(s: RawSocket, on: bool) -> io::Result<()> {
    let val: c_int = i32::from(on);
    let rc = unsafe {
        setsockopt(
            s,
            SOL_SOCKET,
            SO_KEEPALIVE,
            &val as *const _ as *const c_char,
            mem::size_of::<c_int>() as c_int,
        )
    };
    if rc != 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn get_keepalive(s: RawSocket) -> io::Result<bool> {
    let mut val: c_int = 0;
    let mut len = mem::size_of::<c_int>() as c_int;
    let rc = unsafe {
        getsockopt(
            s,
            SOL_SOCKET,
            SO_KEEPALIVE,
            &mut val as *mut _ as *mut c_char,
            &mut len,
        )
    };
    if rc != 0 {
        return Err(last_err());
    }
    Ok(val != 0)
}

fn duration_to_ms(timeout: Option<Duration>) -> u32 {
    match timeout {
        None => 0,
        Some(d) => d.as_millis().min(u32::MAX as u128) as u32,
    }
}

fn ms_to_duration(ms: u32) -> Option<Duration> {
    if ms == 0 {
        None
    } else {
        Some(Duration::from_millis(ms as u64))
    }
}

pub fn set_read_timeout(s: RawSocket, timeout: Option<Duration>) -> io::Result<()> {
    let ms = duration_to_ms(timeout);
    let rc = unsafe {
        setsockopt(
            s,
            SOL_SOCKET,
            SO_RCVTIMEO,
            &ms as *const _ as *const c_char,
            mem::size_of::<u32>() as c_int,
        )
    };
    if rc != 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn get_read_timeout(s: RawSocket) -> io::Result<Option<Duration>> {
    let mut ms: u32 = 0;
    let mut len = mem::size_of::<u32>() as c_int;
    let rc = unsafe {
        getsockopt(
            s,
            SOL_SOCKET,
            SO_RCVTIMEO,
            &mut ms as *mut _ as *mut c_char,
            &mut len,
        )
    };
    if rc != 0 {
        return Err(last_err());
    }
    Ok(ms_to_duration(ms))
}

pub fn set_write_timeout(s: RawSocket, timeout: Option<Duration>) -> io::Result<()> {
    let ms = duration_to_ms(timeout);
    let rc = unsafe {
        setsockopt(
            s,
            SOL_SOCKET,
            SO_SNDTIMEO,
            &ms as *const _ as *const c_char,
            mem::size_of::<u32>() as c_int,
        )
    };
    if rc != 0 {
        return Err(last_err());
    }
    Ok(())
}

pub fn get_write_timeout(s: RawSocket) -> io::Result<Option<Duration>> {
    let mut ms: u32 = 0;
    let mut len = mem::size_of::<u32>() as c_int;
    let rc = unsafe {
        getsockopt(
            s,
            SOL_SOCKET,
            SO_SNDTIMEO,
            &mut ms as *mut _ as *mut c_char,
            &mut len,
        )
    };
    if rc != 0 {
        return Err(last_err());
    }
    Ok(ms_to_duration(ms))
}
