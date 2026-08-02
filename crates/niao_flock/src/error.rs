use std::fmt;
use std::io;
use std::time::Duration;

/// Errors from advisory locking, lockfiles, and PID files.
#[derive(Debug)]
pub enum FlockError {
    Io(io::Error),
    Timeout { path: String, timeout: Duration },
    AlreadyLocked { path: String },
    NotLocked { path: String },
    StaleLock { path: String, pid: u32 },
    LiveLock { path: String, pid: u32 },
    InvalidPid(String),
    InvalidOp(i32),
    Platform(String),
}

impl fmt::Display for FlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FlockError::Io(e) => write!(f, "{e}"),
            FlockError::Timeout { path, timeout } => {
                write!(f, "timed out acquiring lock on {path} after {timeout:?}")
            }
            FlockError::AlreadyLocked { path } => write!(f, "already locked: {path}"),
            FlockError::NotLocked { path } => write!(f, "not locked: {path}"),
            FlockError::StaleLock { path, pid } => {
                write!(f, "removed stale lock {path} (pid {pid} was not running)")
            }
            FlockError::LiveLock { path, pid } => {
                write!(f, "lock held by live process {pid}: {path}")
            }
            FlockError::InvalidPid(s) => write!(f, "invalid pid file contents: {s}"),
            FlockError::InvalidOp(op) => write!(f, "invalid flock operation: {op}"),
            FlockError::Platform(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for FlockError {}

impl From<io::Error> for FlockError {
    fn from(value: io::Error) -> Self {
        FlockError::Io(value)
    }
}

pub type FlockResult<T> = Result<T, FlockError>;
