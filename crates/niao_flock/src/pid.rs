//! PID file helpers for daemon single-instance patterns.

use crate::error::{FlockError, FlockResult};
use crate::lockfile::{break_stale, LockHandle, LockMode, LockOptions};
use crate::sys;
use std::fs;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Options for PID file acquisition.
#[derive(Debug, Clone)]
pub struct PidOptions {
    pub timeout: Option<Duration>,
    pub poll_interval: Duration,
    pub force: bool,
    pub write_pid: bool,
}

impl Default for PidOptions {
    fn default() -> Self {
        Self {
            timeout: None,
            poll_interval: Duration::from_millis(50),
            force: false,
            write_pid: true,
        }
    }
}

/// An acquired PID file with advisory lock held.
pub struct PidFile {
    pub path: PathBuf,
    pub pid: u32,
    lock: LockHandle,
}

impl PidFile {
    /// Acquire PID file at `path`, breaking stale locks when needed.
    pub fn acquire(path: impl AsRef<Path>, opts: &PidOptions) -> FlockResult<Self> {
        let path = path.as_ref().to_path_buf();
        let _ = break_stale(&path, opts.force)?;
        let lock_opts = LockOptions {
            create: true,
            mode: LockMode::Exclusive,
            timeout: opts.timeout,
            poll_interval: opts.poll_interval,
            use_flock: true,
            content: None,
        };
        let mut lock = LockHandle::open(&path, &lock_opts)?;
        if lock.acquire(&lock_opts).is_err() {
            let _ = break_stale(&path, opts.force)?;
            lock = LockHandle::open(&path, &lock_opts)?;
            lock.acquire(&lock_opts)?;
        }
        let pid = std::process::id();
        if opts.write_pid {
            write_pid_to_handle(&mut lock, pid)?;
        }
        Ok(Self { path, pid, lock })
    }

    /// Release lock and remove PID file.
    pub fn release(mut self) -> FlockResult<()> {
        let path = self.path.clone();
        self.lock.release()?;
        let _ = fs::remove_file(path);
        Ok(())
    }

    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Write `pid` (default: current process) to `path` without locking.
pub fn write_pid(path: impl AsRef<Path>, pid: Option<u32>) -> FlockResult<()> {
    write_pid_bytes(path, pid.unwrap_or_else(std::process::id))
}

fn write_pid_to_handle(lock: &mut LockHandle, pid: u32) -> FlockResult<()> {
    let file = lock.file_mut();
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    writeln!(file, "{pid}")?;
    file.sync_all()?;
    Ok(())
}

fn write_pid_bytes(path: impl AsRef<Path>, pid: u32) -> FlockResult<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    writeln!(file, "{pid}")?;
    file.sync_all()?;
    Ok(())
}

/// Read PID from `path`; returns catchable error when missing or malformed.
pub fn read_pid(path: impl AsRef<Path>) -> FlockResult<u32> {
    let bytes = fs::read(path)?;
    let s = std::str::from_utf8(&bytes)
        .map_err(|_| FlockError::InvalidPid(String::from_utf8_lossy(&bytes).into_owned()))?
        .trim();
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits
        .parse::<u32>()
        .map_err(|_| FlockError::InvalidPid(s.to_string()))
}

/// Remove PID file when present (no error if missing).
pub fn remove_pid(path: impl AsRef<Path>) -> FlockResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Whether `pid` is a live process.
#[inline]
pub fn pid_alive(pid: u32) -> bool {
    sys::pid_alive(pid)
}
