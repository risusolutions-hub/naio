//! Bounded-concurrency process pool — dispatches argv batches to child processes.

use crate::process::{run_output, SpawnOpts};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

#[derive(Clone, Debug)]
pub struct JobResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub code: i32,
    pub ok: bool,
}

struct PoolInner {
    workers: usize,
    closed: Mutex<bool>,
    active: AtomicUsize,
    gate: (Mutex<usize>, Condvar),
}

pub struct ProcessPool {
    inner: Arc<PoolInner>,
}

impl ProcessPool {
    pub fn new(workers: usize) -> Self {
        let n = workers.max(1);
        Self {
            inner: Arc::new(PoolInner {
                workers: n,
                closed: Mutex::new(false),
                active: AtomicUsize::new(0),
                gate: (Mutex::new(0), Condvar::new()),
            }),
        }
    }

    pub fn workers(&self) -> usize {
        self.inner.workers
    }

    pub fn active(&self) -> usize {
        self.inner.active.load(Ordering::Relaxed)
    }

    pub fn close(&self) {
        *self.inner.closed.lock().unwrap() = true;
    }

    pub fn is_closed(&self) -> bool {
        *self.inner.closed.lock().unwrap()
    }

    fn acquire_slot(inner: &PoolInner) {
        let (lock, cv) = &inner.gate;
        let mut in_flight = lock.lock().unwrap();
        while *in_flight >= inner.workers {
            in_flight = cv.wait(in_flight).unwrap();
        }
        *in_flight += 1;
        inner.active.fetch_add(1, Ordering::Relaxed);
    }

    fn release_slot(inner: &PoolInner) {
        let (lock, cv) = &inner.gate;
        let mut in_flight = lock.lock().unwrap();
        *in_flight = in_flight.saturating_sub(1);
        inner.active.fetch_sub(1, Ordering::Relaxed);
        cv.notify_one();
    }

    /// Run each argv vector as a subprocess; results preserve input order.
    pub fn map(
        &self,
        commands: &[Vec<String>],
        opts: &SpawnOpts,
    ) -> Result<Vec<JobResult>, String> {
        if self.is_closed() {
            return Err("process pool is closed".into());
        }
        if commands.is_empty() {
            return Ok(Vec::new());
        }
        let n = commands.len();
        let results = Arc::new(Mutex::new((0..n).map(|_| None).collect::<Vec<_>>()));
        let inner = Arc::clone(&self.inner);
        thread::scope(|scope| {
            let mut handles = Vec::with_capacity(n);
            for (i, argv) in commands.iter().enumerate() {
                if argv.is_empty() {
                    return Err("command argv must not be empty".to_string());
                }
                let program = argv[0].clone();
                let args: Vec<String> = argv[1..].to_vec();
                let opts = opts.clone();
                let results = Arc::clone(&results);
                let inner = Arc::clone(&inner);
                handles.push(scope.spawn(move || {
                    Self::acquire_slot(&inner);
                    let job = match run_output(&program, &args, &opts) {
                        Ok((stdout, stderr, status)) => JobResult {
                            ok: status.success(),
                            code: status.code().unwrap_or(-1),
                            stdout,
                            stderr,
                        },
                        Err(e) => JobResult {
                            ok: false,
                            code: -1,
                            stdout: Vec::new(),
                            stderr: e.to_string().into_bytes(),
                        },
                    };
                    results.lock().unwrap()[i] = Some(job);
                    Self::release_slot(&inner);
                }));
            }
            for h in handles {
                h.join().map_err(|_| "pool worker panicked".to_string())?;
            }
            Ok(())
        })?;
        let out = results
            .lock()
            .unwrap()
            .drain(..)
            .map(|o| o.expect("pool result slot"))
            .collect();
        Ok(out)
    }

    /// Append each `item` as the final argv element to `template`.
    pub fn map_argv(
        &self,
        template: &[String],
        items: &[String],
        opts: &SpawnOpts,
    ) -> Result<Vec<JobResult>, String> {
        if template.is_empty() {
            return Err("argv template must not be empty".into());
        }
        let commands: Vec<Vec<String>> = items
            .iter()
            .map(|item| {
                let mut argv = template.to_vec();
                argv.push(item.clone());
                argv
            })
            .collect();
        self.map(&commands, opts)
    }

    pub fn join(&self) -> Result<(), String> {
        let (lock, cv) = &self.inner.gate;
        let mut in_flight = lock.lock().unwrap();
        while *in_flight > 0 {
            in_flight = cv.wait(in_flight).unwrap();
        }
        Ok(())
    }
}
