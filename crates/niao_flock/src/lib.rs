//! `niao_flock` — advisory file locks, lockfiles, PID files, and timeouts.
//!
//! Cross-platform locking via `flock` (Unix), `LockFileEx` (Windows), and
//! POSIX `fcntl` record locks through [`fs2`] when `use_flock` is disabled.

pub mod error;
pub mod lockfile;
pub mod pid;
pub mod sys;

pub use error::{FlockError, FlockResult};
pub use lockfile::{break_stale, lock, read_lock_pid, LockHandle, LockMode, LockOptions};
pub use pid::{pid_alive, read_pid, remove_pid, write_pid, PidFile, PidOptions};
pub use sys::{
    acquire_with_timeout, flock, lockf, pid_alive as sys_pid_alive, F_GETLK, F_RDLCK, F_SETLK,
    F_SETLKW, F_UNLCK, F_WRLCK, LOCK_EX, LOCK_NB, LOCK_SH, LOCK_UN,
};
