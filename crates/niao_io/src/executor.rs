//! Work-stealing thread pool executor (callback model).

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;

type Job = Box<dyn FnOnce() + Send + 'static>;

struct Worker {
    queue: Mutex<VecDeque<Job>>,
    notify: Condvar,
}

struct ExecutorInner {
    workers: Vec<Arc<Worker>>,
    global: Mutex<VecDeque<Job>>,
    global_notify: Condvar,
}

pub struct Executor {
    inner: Arc<ExecutorInner>,
}

static GLOBAL: OnceLock<Executor> = OnceLock::new();

impl Executor {
    pub fn global() -> &'static Executor {
        GLOBAL.get_or_init(Executor::new)
    }

    pub fn new() -> Self {
        let n = thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4)
            .clamp(2, 16);
        let workers: Vec<Arc<Worker>> = (0..n)
            .map(|_| {
                Arc::new(Worker {
                    queue: Mutex::new(VecDeque::new()),
                    notify: Condvar::new(),
                })
            })
            .collect();
        let inner = Arc::new(ExecutorInner {
            workers: workers.clone(),
            global: Mutex::new(VecDeque::new()),
            global_notify: Condvar::new(),
        });
        for (i, worker) in workers.iter().enumerate() {
            let inner_c = Arc::clone(&inner);
            let local = Arc::clone(worker);
            thread::Builder::new()
                .name(format!("niao-io-{i}"))
                .spawn(move || worker_loop(i, inner_c, local))
                .expect("spawn worker");
        }
        Self { inner }
    }

    pub fn spawn<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let n = self.inner.workers.len();
        if n == 0 {
            self.inner.global.lock().unwrap().push_back(Box::new(f));
            self.inner.global_notify.notify_one();
            return;
        }
        let idx = fastrand() % n;
        let w = &self.inner.workers[idx];
        w.queue.lock().unwrap().push_back(Box::new(f));
        w.notify.notify_one();
    }
}

fn fastrand() -> usize {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as usize)
        .unwrap_or(0)
}

fn worker_loop(worker_id: usize, inner: Arc<ExecutorInner>, local: Arc<Worker>) {
    loop {
        if let Some(job) = local.queue.lock().unwrap().pop_front() {
            job();
            continue;
        }
        if let Some(job) = inner.global.lock().unwrap().pop_front() {
            job();
            continue;
        }
        let steal_from = (worker_id + 1) % inner.workers.len();
        if steal_from != worker_id {
            if let Some(job) = inner.workers[steal_from].queue.lock().unwrap().pop_front() {
                job();
                continue;
            }
        }
        let _ = local
            .notify
            .wait_timeout(local.queue.lock().unwrap(), std::time::Duration::from_millis(100))
            .unwrap();
    }
}

pub fn spawn<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    Executor::global().spawn(f);
}
