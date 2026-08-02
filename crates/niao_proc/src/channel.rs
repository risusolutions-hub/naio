//! In-process bounded and unbounded channels (thread IPC).

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

pub struct Channel<T> {
    cap: Option<usize>,
    queue: Mutex<VecDeque<T>>,
    cv: Condvar,
    closed: Mutex<bool>,
}

impl<T> Channel<T> {
    pub fn bounded(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            cap: Some(cap),
            queue: Mutex::new(VecDeque::with_capacity(cap)),
            cv: Condvar::new(),
            closed: Mutex::new(false),
        }
    }

    pub fn unbounded() -> Self {
        Self {
            cap: None,
            queue: Mutex::new(VecDeque::new()),
            cv: Condvar::new(),
            closed: Mutex::new(false),
        }
    }

    pub fn capacity(&self) -> Option<usize> {
        self.cap
    }

    pub fn close(&self) {
        *self.closed.lock().unwrap() = true;
        self.cv.notify_all();
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.lock().unwrap()
    }

    pub fn send(&self, value: T) -> Result<(), String> {
        if *self.closed.lock().unwrap() {
            return Err("channel closed".into());
        }
        let mut q = self.queue.lock().unwrap();
        if let Some(cap) = self.cap {
            while q.len() >= cap && !*self.closed.lock().unwrap() {
                q = self.cv.wait(q).unwrap();
            }
            if *self.closed.lock().unwrap() {
                return Err("channel closed".into());
            }
        }
        q.push_back(value);
        self.cv.notify_one();
        Ok(())
    }

    pub fn try_send(&self, value: T) -> Result<bool, String> {
        if *self.closed.lock().unwrap() {
            return Err("channel closed".into());
        }
        let mut q = self.queue.lock().unwrap();
        if let Some(cap) = self.cap {
            if q.len() >= cap {
                return Ok(false);
            }
        }
        q.push_back(value);
        self.cv.notify_one();
        Ok(true)
    }

    pub fn recv(&self, timeout_ms: Option<u64>) -> Result<Option<T>, String> {
        let mut q = self.queue.lock().unwrap();
        loop {
            if let Some(v) = q.pop_front() {
                self.cv.notify_one();
                return Ok(Some(v));
            }
            if *self.closed.lock().unwrap() {
                return Ok(None);
            }
            match timeout_ms {
                None => q = self.cv.wait(q).unwrap(),
                Some(0) => return Ok(None),
                Some(ms) => {
                    let (q2, to) = self.cv.wait_timeout(q, Duration::from_millis(ms)).unwrap();
                    q = q2;
                    if to.timed_out() {
                        return Ok(None);
                    }
                }
            }
        }
    }

    pub fn try_recv(&self) -> Result<Option<T>, String> {
        let mut q = self.queue.lock().unwrap();
        if let Some(v) = q.pop_front() {
            self.cv.notify_one();
            Ok(Some(v))
        } else if *self.closed.lock().unwrap() {
            Ok(None)
        } else {
            Ok(None)
        }
    }

    pub fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }
}

pub type SharedChannel<T> = Arc<Channel<T>>;
