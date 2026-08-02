//! Platform flock / lockf primitives and process liveness checks.

use crate::error::{FlockError, FlockResult};
use std::fs::File;
use std::io;
use std::time::{Duration, Instant};

/// `fcntl.flock` / `LOCK_*` constants.
pub const LOCK_SH: i32 = 1;
pub const LOCK_EX: i32 = 2;
pub const LOCK_NB: i32 = 4;
pub const LOCK_UN: i32 = 8;

/// `fcntl.lockf` record-lock types.
pub const F_RDLCK: i16 = 1;
pub const F_WRLCK: i16 = 2;
pub const F_UNLCK: i16 = 3;

/// `fcntl.lockf` commands (Linux values; mapped on other Unix).
pub const F_GETLK: i32 = 5;
pub const F_SETLK: i32 = 6;
pub const F_SETLKW: i32 = 7;

/// Apply a BSD-style `flock` operation to an open file descriptor.
pub fn flock(file: &File, op: i32) -> FlockResult<()> {
    validate_flock_op(op)?;
    flock_impl(file, op)
}

/// POSIX record lock via `fcntl` / `lockf` semantics.
pub fn lockf(file: &File, cmd: i32, len: i64, start: i64) -> FlockResult<()> {
    lockf_impl(file, cmd, len, start)
}

/// Poll `try_acquire` until success, timeout, or unrecoverable error.
pub fn acquire_with_timeout<F>(
    timeout: Option<Duration>,
    poll: Duration,
    mut try_fn: F,
) -> FlockResult<()>
where
    F: FnMut() -> FlockResult<bool>,
{
    let deadline = timeout.map(|t| Instant::now() + t);
    loop {
        match try_fn()? {
            true => return Ok(()),
            false => {
                if let Some(dl) = deadline {
                    if Instant::now() >= dl {
                        return Err(FlockError::Io(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "lock timeout",
                        )));
                    }
                    let remaining = dl.saturating_duration_since(Instant::now());
                    std::thread::sleep(poll.min(remaining));
                } else {
                    std::thread::sleep(poll);
                }
            }
        }
    }
}

fn validate_flock_op(op: i32) -> FlockResult<()> {
    let base = op & !LOCK_NB;
    if matches!(base, LOCK_SH | LOCK_EX | LOCK_UN) {
        Ok(())
    } else {
        Err(FlockError::InvalidOp(op))
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::io::AsRawFd;

    pub fn flock_impl(file: &File, op: i32) -> FlockResult<()> {
        let fd = file.as_raw_fd();
        let rc = unsafe { libc::flock(fd, op) };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error().into())
        }
    }

    #[repr(C)]
    struct Flock {
        l_type: i16,
        l_whence: i16,
        l_start: i64,
        l_len: i64,
        l_pid: i32,
    }

    pub fn lockf_impl(file: &File, cmd: i32, len: i64, start: i64) -> FlockResult<()> {
        let fd = file.as_raw_fd();
        let mut fl = Flock {
            l_type: F_WRLCK,
            l_whence: libc::SEEK_SET as i16,
            l_start: start,
            l_len: len,
            l_pid: 0,
        };

        match cmd {
            F_GETLK => {
                let rc = unsafe { libc::fcntl(fd, F_GETLK, &mut fl) };
                if rc == -1 {
                    return Err(io::Error::last_os_error().into());
                }
                if fl.l_type == F_UNLCK {
                    Ok(())
                } else {
                    Err(FlockError::AlreadyLocked {
                        path: format!("fd={fd}"),
                    })
                }
            }
            F_SETLK => setlk(fd, &mut fl, false),
            F_SETLKW => setlk(fd, &mut fl, true),
            other => Err(FlockError::InvalidOp(other)),
        }
    }

    fn setlk(fd: i32, fl: &mut Flock, wait: bool) -> FlockResult<()> {
        let cmd = if wait { F_SETLKW } else { F_SETLK };
        let rc = unsafe { libc::fcntl(fd, cmd, fl) };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error().into())
        }
    }
}

#[cfg(unix)]
use unix::{flock_impl, lockf_impl};

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use std::ptr;

    const ERROR_LOCK_VIOLATION: u32 = 33;
    const LOCKFILE_FAIL_IMMEDIATELY: u32 = 0x0000_0001;

    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        h_event: *mut core::ffi::c_void,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LockFileEx(
            h_file: *mut core::ffi::c_void,
            dw_flags: u32,
            dw_reserved: u32,
            n_number_of_bytes_to_lock_low: u32,
            n_number_of_bytes_to_lock_high: u32,
            lp_overlapped: *mut Overlapped,
        ) -> i32;
        fn UnlockFile(
            h_file: *mut core::ffi::c_void,
            dw_file_offset_low: u32,
            dw_file_offset_high: u32,
            n_number_of_bytes_to_unlock_low: u32,
            n_number_of_bytes_to_unlock_high: u32,
        ) -> i32;
        fn OpenProcess(
            dw_desired_access: u32,
            b_inherit_handle: i32,
            dw_process_id: u32,
        ) -> *mut core::ffi::c_void;
        fn GetExitCodeProcess(h_process: *mut core::ffi::c_void, lp_exit_code: *mut u32) -> i32;
        fn CloseHandle(h_object: *mut core::ffi::c_void) -> i32;
    }

    pub fn flock_impl(file: &File, op: i32) -> FlockResult<()> {
        let handle = file.as_raw_handle() as *mut core::ffi::c_void;
        let base = op & !LOCK_NB;
        match base {
            LOCK_UN => {
                if unsafe { UnlockFile(handle, 0, 0, u32::MAX, u32::MAX) } == 0 {
                    return Err(io::Error::last_os_error().into());
                }
                Ok(())
            }
            LOCK_SH | LOCK_EX => {
                let flags = if (op & LOCK_NB) != 0 {
                    LOCKFILE_FAIL_IMMEDIATELY
                } else {
                    0
                };
                let mut ov = Overlapped {
                    internal: 0,
                    internal_high: 0,
                    offset: 0,
                    offset_high: 0,
                    h_event: ptr::null_mut(),
                };
                if unsafe { LockFileEx(handle, flags, 0, u32::MAX, u32::MAX, &mut ov) } == 0 {
                    return Err(io::Error::last_os_error().into());
                }
                Ok(())
            }
            _ => Err(FlockError::InvalidOp(op)),
        }
    }

    pub fn lockf_impl(file: &File, cmd: i32, _len: i64, _start: i64) -> FlockResult<()> {
        match cmd {
            F_SETLK => flock_impl(file, LOCK_EX | LOCK_NB),
            F_SETLKW => flock_impl(file, LOCK_EX),
            F_GETLK => match flock_impl(file, LOCK_EX | LOCK_NB) {
                Ok(()) => flock_impl(file, LOCK_UN),
                Err(FlockError::Io(e)) if e.raw_os_error() == Some(ERROR_LOCK_VIOLATION as i32) => {
                    Err(FlockError::AlreadyLocked {
                        path: "windows".into(),
                    })
                }
                Err(e) => Err(e),
            },
            other => Err(FlockError::InvalidOp(other)),
        }
    }

    pub fn pid_alive(pid: u32) -> bool {
        if pid == 0 {
            return false;
        }
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const STILL_ACTIVE: u32 = 259;
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return false;
            }
            let mut code = 0u32;
            let ok = GetExitCodeProcess(h, &mut code);
            CloseHandle(h);
            ok != 0 && code == STILL_ACTIVE
        }
    }
}

#[cfg(windows)]
use windows::{flock_impl, lockf_impl};

#[cfg(not(any(unix, windows)))]
mod fallback {
    use super::*;
    use fs2::FileExt;

    pub fn flock_impl(file: &File, op: i32) -> FlockResult<()> {
        let base = op & !LOCK_NB;
        let nb = (op & LOCK_NB) != 0;
        match base {
            LOCK_SH => {
                if nb {
                    file.try_lock_shared().map(|_| ()).map_err(Into::into)
                } else {
                    file.lock_shared().map_err(Into::into)
                }
            }
            LOCK_EX => {
                if nb {
                    file.try_lock_exclusive().map(|_| ()).map_err(Into::into)
                } else {
                    file.lock_exclusive().map_err(Into::into)
                }
            }
            LOCK_UN => file.unlock().map_err(Into::into),
            _ => Err(FlockError::InvalidOp(op)),
        }
    }

    pub fn lockf_impl(file: &File, cmd: i32, _len: i64, _start: i64) -> FlockResult<()> {
        flock_impl(
            file,
            match cmd {
                F_SETLK => LOCK_EX | LOCK_NB,
                F_SETLKW => LOCK_EX,
                F_GETLK => LOCK_EX | LOCK_NB,
                _ => return Err(FlockError::InvalidOp(cmd)),
            },
        )
    }
}

#[cfg(not(any(unix, windows)))]
use fallback::{flock_impl, lockf_impl};

/// Return whether `pid` refers to a live process on this host.
pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(pid as i32, 0) == 0
                || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        }
    }
    #[cfg(windows)]
    {
        windows::pid_alive(pid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}
