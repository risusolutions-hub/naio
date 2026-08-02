//! `niao_proc` — child processes, pools, OS pipes, channels, shared memory, and
//! in-process sync primitives. Zero third-party dependencies beyond `niao_parallel`
//! (used by the process pool dispatcher).

pub mod channel;
pub mod os_pipe;
pub mod pool;
pub mod process;
pub mod shm;
pub mod sync;

pub use channel::{Channel, SharedChannel};
pub use os_pipe::OsPipe;
pub use pool::{JobResult, ProcessPool};
pub use process::{ChildProcess, SpawnOpts};
pub use shm::SharedMemory;
pub use sync::{
    Barrier, Event, Lock, Semaphore, SharedBarrier, SharedEvent, SharedLock, SharedSemaphore,
};

/// Logical CPU count (minimum 1).
pub fn cpu_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}
