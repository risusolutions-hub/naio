//! High-level lockfiles and advisory file handles (~Python `filelock`).

use crate::error::{FlockError, FlockResult};
use crate::sys::{self, acquire_with_timeout, LOCK_EX, LOCK_NB, LOCK_SH, LOCK_UN};
use fs2::FileExt;
use std::fs::File;
#[cfg(not(windows))]
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Shared vs exclusive advisory lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    Shared,
    Exclusive,
}

impl LockMode {
    pub fn flock_op(self, non_blocking: bool) -> i32 {
        let base = match self {
            LockMode::Shared => LOCK_SH,
            LockMode::Exclusive => LOCK_EX,
        };
        if non_blocking {
            base | LOCK_NB
        } else {
            base
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "shared" | "sh" | "read" => Some(LockMode::Shared),
            "exclusive" | "ex" | "write" => Some(LockMode::Exclusive),
            _ => None,
        }
    }
}

/// Options for opening and acquiring locks.
#[derive(Debug, Clone)]
pub struct LockOptions {
    pub create: bool,
    pub mode: LockMode,
    pub timeout: Option<Duration>,
    pub poll_interval: Duration,
    pub use_flock: bool,
    pub content: Option<String>,
}

impl Default for LockOptions {
    fn default() -> Self {
        Self {
            create: true,
            mode: LockMode::Exclusive,
            timeout: None,
            poll_interval: Duration::from_millis(50),
            use_flock: true,
            content: None,
        }
    }
}

/// Open lock file backing an advisory lock.
pub struct LockHandle {
    pub path: PathBuf,
    file: File,
    locked: bool,
    mode: Option<LockMode>,
    use_flock: bool,
    /// Windows: `CreateFileW` with `dwShareMode = 0` already excludes other openers.
    os_exclusive: bool,
}

impl LockHandle {
    /// Open `path` without acquiring a lock.
    pub fn open(path: impl AsRef<Path>, opts: &LockOptions) -> FlockResult<Self> {
        let path = path.as_ref().to_path_buf();
        let (file, os_exclusive) = open_lock_file(&path, opts.create, opts.mode)?;
        Ok(Self {
            path,
            file,
            locked: false,
            mode: None,
            use_flock: opts.use_flock,
            os_exclusive,
        })
    }

    /// Writable file handle (e.g. write PID into a held lock).
    pub fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    /// Acquire advisory lock using configured mode and timeout.
    pub fn acquire(&mut self, opts: &LockOptions) -> FlockResult<()> {
        if self.locked {
            return Err(FlockError::AlreadyLocked {
                path: self.path.display().to_string(),
            });
        }
        if opts.timeout.is_some() {
            acquire_with_timeout(opts.timeout, opts.poll_interval, || {
                self.try_acquire_inner(opts.mode)
            })?;
        } else {
            self.acquire_blocking(opts.mode)?;
        }
        if let Some(content) = &opts.content {
            self.file.set_len(0)?;
            self.file.write_all(content.as_bytes())?;
            self.file.sync_all()?;
        }
        self.locked = true;
        self.mode = Some(opts.mode);
        self.use_flock = opts.use_flock;
        Ok(())
    }

    /// Non-blocking acquire; returns `true` when the lock is held.
    pub fn try_acquire(&mut self, mode: LockMode) -> FlockResult<bool> {
        if self.locked {
            return Err(FlockError::AlreadyLocked {
                path: self.path.display().to_string(),
            });
        }
        if self.try_acquire_inner(mode)? {
            self.locked = true;
            self.mode = Some(mode);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn acquire_blocking(&mut self, mode: LockMode) -> FlockResult<()> {
        if self.os_exclusive && mode == LockMode::Exclusive {
            return Ok(());
        }
        if self.use_flock {
            sys::flock(&self.file, mode.flock_op(false))?;
        } else {
            match mode {
                LockMode::Shared => self.file.lock_shared()?,
                LockMode::Exclusive => self.file.lock_exclusive()?,
            }
        }
        Ok(())
    }

    fn try_acquire_inner(&mut self, mode: LockMode) -> FlockResult<bool> {
        if self.os_exclusive && mode == LockMode::Exclusive {
            return Ok(true);
        }
        if self.use_flock {
            match sys::flock(&self.file, mode.flock_op(true)) {
                Ok(()) => Ok(true),
                Err(FlockError::Io(e)) if is_would_block(&e) => Ok(false),
                Err(e) => Err(e),
            }
        } else {
            match mode {
                LockMode::Shared => match self.file.try_lock_shared() {
                    Ok(()) => Ok(true),
                    Err(std::fs::TryLockError::WouldBlock) => Ok(false),
                    Err(std::fs::TryLockError::Error(e)) => {
                        if is_would_block(&e) {
                            Ok(false)
                        } else {
                            Err(e.into())
                        }
                    }
                },
                LockMode::Exclusive => match self.file.try_lock_exclusive() {
                    Ok(()) => Ok(true),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
                    Err(e) => Err(e.into()),
                },
            }
        }
    }

    /// Release advisory lock; keeps file open.
    pub fn release(&mut self) -> FlockResult<()> {
        if !self.locked {
            return Err(FlockError::NotLocked {
                path: self.path.display().to_string(),
            });
        }
        if self.os_exclusive {
            self.locked = false;
            self.mode = None;
            return Ok(());
        }
        if self.use_flock {
            sys::flock(&self.file, LOCK_UN)?;
        } else {
            self.file.unlock()?;
        }
        self.locked = false;
        self.mode = None;
        Ok(())
    }

    /// Whether this handle currently holds a lock.
    #[inline]
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Underlying open file descriptor (for `flock` / `lockf`).
    #[inline]
    pub fn file(&self) -> &File {
        &self.file
    }

    /// Path to the lock file.
    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Current lock mode when held.
    #[inline]
    pub fn mode(&self) -> Option<LockMode> {
        self.mode
    }
}

impl Drop for LockHandle {
    fn drop(&mut self) {
        if self.locked {
            let _ = if self.use_flock {
                sys::flock(&self.file, LOCK_UN)
            } else {
                self.file.unlock().map_err(Into::into)
            };
        }
    }
}

fn is_would_block(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::WouldBlock {
        return true;
    }
    #[cfg(unix)]
    {
        matches!(
            e.raw_os_error(),
            Some(libc::EWOULDBLOCK) | Some(libc::EAGAIN)
        )
    }
    #[cfg(windows)]
    {
        matches!(
            e.raw_os_error(),
            Some(33) | Some(32) // LOCK_VIOLATION | SHARING_VIOLATION
        )
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = e;
        false
    }
}

fn open_lock_file(path: &Path, create: bool, mode: LockMode) -> FlockResult<(File, bool)> {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::io::FromRawHandle;

        #[link(name = "kernel32")]
        extern "system" {
            fn CreateFileW(
                lp_file_name: *const u16,
                dw_desired_access: u32,
                dw_share_mode: u32,
                lp_security_attributes: *mut core::ffi::c_void,
                dw_creation_disposition: u32,
                dw_flags_and_attributes: u32,
                h_template_file: *mut core::ffi::c_void,
            ) -> *mut core::ffi::c_void;
        }

        const GENERIC_READ: u32 = 0x8000_0000;
        const GENERIC_WRITE: u32 = 0x4000_0000;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        const FILE_SHARE_WRITE: u32 = 0x0000_0002;
        const OPEN_EXISTING: u32 = 3;
        const OPEN_ALWAYS: u32 = 4;
        const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
        const INVALID_HANDLE_VALUE: *mut core::ffi::c_void = -1isize as *mut core::ffi::c_void;

        let wide: Vec<u16> = OsStr::new(path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let share = match mode {
            LockMode::Shared => FILE_SHARE_READ | FILE_SHARE_WRITE,
            LockMode::Exclusive => 0,
        };
        let creation = if create { OPEN_ALWAYS } else { OPEN_EXISTING };
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                share,
                std::ptr::null_mut(),
                creation,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().into());
        }
        let os_exclusive = mode == LockMode::Exclusive;
        Ok((unsafe { File::from_raw_handle(handle as _) }, os_exclusive))
    }
    #[cfg(not(windows))]
    {
        let mut oo = OpenOptions::new();
        oo.read(true).write(true);
        if create {
            oo.create(true);
        }
        Ok((oo.open(path)?, false))
    }
}

/// Convenience: open, acquire, return handle (~`FileLock(...).acquire()`).
pub fn lock(path: impl AsRef<Path>, opts: &LockOptions) -> FlockResult<LockHandle> {
    let path_buf = path.as_ref().to_path_buf();
    let try_once = || -> FlockResult<Option<LockHandle>> {
        let mut h = match LockHandle::open(&path_buf, opts) {
            Ok(h) => h,
            Err(FlockError::Io(e)) if is_would_block(&e) => return Ok(None),
            Err(e) => return Err(e),
        };
        if h.try_acquire_inner(opts.mode)? {
            if let Some(content) = &opts.content {
                h.file_mut().set_len(0)?;
                h.file_mut().write_all(content.as_bytes())?;
                h.file_mut().sync_all()?;
            }
            h.locked = true;
            h.mode = Some(opts.mode);
            Ok(Some(h))
        } else {
            Ok(None)
        }
    };

    if opts.timeout.is_some() {
        let mut acquired: Option<LockHandle> = None;
        acquire_with_timeout(opts.timeout, opts.poll_interval, || {
            if let Some(h) = try_once()? {
                acquired = Some(h);
                Ok(true)
            } else {
                Ok(false)
            }
        })?;
        acquired.ok_or_else(|| FlockError::Timeout {
            path: path_buf.display().to_string(),
            timeout: opts.timeout.unwrap(),
        })
    } else {
        loop {
            if let Some(h) = try_once()? {
                return Ok(h);
            }
            std::thread::sleep(opts.poll_interval);
        }
    }
}

/// Read optional PID integer from a lockfile's first line.
pub fn read_lock_pid(path: impl AsRef<Path>) -> FlockResult<Option<u32>> {
    let path = path.as_ref();
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut buf = [0u8; 32];
    let n = file.read(&mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    let s = std::str::from_utf8(&buf[..n])
        .map_err(|_| FlockError::InvalidPid(String::from_utf8_lossy(&buf[..n]).into_owned()))?
        .trim();
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return Err(FlockError::InvalidPid(s.to_string()));
    }
    digits
        .parse::<u32>()
        .map(Some)
        .map_err(|_| FlockError::InvalidPid(digits))
}

/// Break a stale lock when the recorded PID is not alive.
pub fn break_stale(path: impl AsRef<Path>, force: bool) -> FlockResult<bool> {
    let path = path.as_ref();
    if let Some(pid) = read_lock_pid(path)? {
        if sys::pid_alive(pid) && !force {
            return Err(FlockError::LiveLock {
                path: path.display().to_string(),
                pid,
            });
        }
        std::fs::remove_file(path)?;
        return Ok(true);
    }
    if path.exists() && force {
        std::fs::remove_file(path)?;
        return Ok(true);
    }
    Ok(false)
}
