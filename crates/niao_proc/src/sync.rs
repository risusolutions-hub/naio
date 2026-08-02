//! In-process synchronization primitives (Event, Lock, Semaphore, Barrier).

use std::sync::{Arc, Barrier as StdBarrier, Condvar, Mutex};

pub struct Event {
    flag: Mutex<bool>,
    cv: Condvar,
}

impl Event {
    pub fn new() -> Self {
        Self {
            flag: Mutex::new(false),
            cv: Condvar::new(),
        }
    }

    pub fn set(&self) {
        let mut f = self.flag.lock().unwrap();
        *f = true;
        self.cv.notify_all();
    }

    pub fn clear(&self) {
        *self.flag.lock().unwrap() = false;
    }

    pub fn is_set(&self) -> bool {
        *self.flag.lock().unwrap()
    }

    pub fn wait(&self, timeout_ms: Option<u64>) -> bool {
        let mut f = self.flag.lock().unwrap();
        if *f {
            return true;
        }
        match timeout_ms {
            None => {
                while !*f {
                    f = self.cv.wait(f).unwrap();
                }
                true
            }
            Some(0) => false,
            Some(ms) => {
                let (f2, timeout) = self
                    .cv
                    .wait_timeout(f, std::time::Duration::from_millis(ms))
                    .unwrap();
                f = f2;
                !timeout.timed_out() || *f
            }
        }
    }
}

/// Mutex-style lock (non-reentrant).
pub struct Lock {
    inner: Mutex<()>,
}

impl Lock {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(()),
        }
    }

    pub fn acquire(&self, timeout_ms: Option<u64>) -> bool {
        match timeout_ms {
            None => self.inner.lock().is_ok(),
            Some(0) => self.try_acquire(),
            Some(ms) => {
                let start = std::time::Instant::now();
                loop {
                    if self.inner.try_lock().is_ok() {
                        return true;
                    }
                    if start.elapsed().as_millis() as u64 >= ms {
                        return false;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        }
    }

    pub fn try_acquire(&self) -> bool {
        self.inner.try_lock().is_ok()
    }

    pub fn release(&self) {
        // Guard drops immediately after try_lock/acquire in this simplified model;
        // runtime bindings keep the guard in thread-local storage.
    }
}

impl Lock {
    pub fn lock_inner(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.inner.lock().map_err(|e| format!("lock poisoned: {e}"))
    }

    pub fn try_lock_inner(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.inner
            .try_lock()
            .map_err(|e| format!("lock unavailable: {e}"))
    }
}

pub struct Semaphore {
    permits: Mutex<usize>,
    cv: Condvar,
    capacity: usize,
}

impl Semaphore {
    pub fn new(permits: usize) -> Self {
        Self {
            permits: Mutex::new(permits),
            cv: Condvar::new(),
            capacity: permits,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn available(&self) -> usize {
        *self.permits.lock().unwrap()
    }

    pub fn acquire(&self, timeout_ms: Option<u64>) -> bool {
        let mut p = self.permits.lock().unwrap();
        if *p > 0 {
            *p -= 1;
            return true;
        }
        match timeout_ms {
            None => {
                while *p == 0 {
                    p = self.cv.wait(p).unwrap();
                }
                *p -= 1;
                true
            }
            Some(0) => false,
            Some(ms) => {
                let start = std::time::Instant::now();
                while *p == 0 {
                    let elapsed = start.elapsed().as_millis() as u64;
                    if elapsed >= ms {
                        return false;
                    }
                    let remain = ms - elapsed;
                    let (p2, to) = self
                        .cv
                        .wait_timeout(p, std::time::Duration::from_millis(remain))
                        .unwrap();
                    p = p2;
                    if to.timed_out() && *p == 0 {
                        return false;
                    }
                }
                *p -= 1;
                true
            }
        }
    }

    pub fn try_acquire(&self) -> bool {
        let mut p = self.permits.lock().unwrap();
        if *p > 0 {
            *p -= 1;
            true
        } else {
            false
        }
    }

    pub fn release(&self) {
        let mut p = self.permits.lock().unwrap();
        if *p < self.capacity {
            *p += 1;
            self.cv.notify_one();
        }
    }
}

pub struct Barrier {
    inner: StdBarrier,
}

impl Barrier {
    pub fn new(parties: usize) -> Result<Self, String> {
        if parties == 0 {
            return Err("barrier parties must be > 0".into());
        }
        Ok(Self {
            inner: StdBarrier::new(parties),
        })
    }

    pub fn wait(&self, timeout_ms: Option<u64>) -> Result<usize, String> {
        if timeout_ms.is_some() {
            return Err("barrier does not support timed wait on this platform".into());
        }
        let r = self.inner.wait();
        Ok(if r.is_leader() { 0 } else { 1 })
    }
}

pub type SharedEvent = Arc<Event>;
pub type SharedLock = Arc<Lock>;
pub type SharedSemaphore = Arc<Semaphore>;
pub type SharedBarrier = Arc<Barrier>;
