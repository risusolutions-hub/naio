//! Generic connection pool (replaces r2d2).

use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum PoolError {
    Timeout,
    Connect(String),
}

impl std::fmt::Display for PoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PoolError::Timeout => write!(f, "pool connection timeout"),
            PoolError::Connect(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for PoolError {}

pub trait ManageConnection: Send + Sync + 'static {
    type Connection: Send;
    fn connect(&self) -> Result<Self::Connection, String>;
    fn is_valid(&self, conn: &Self::Connection) -> Result<(), String> {
        let _ = conn;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PoolState {
    pub connections: u32,
    pub idle_connections: u32,
}

struct IdleEntry<C> {
    conn: C,
    created: Instant,
}

struct Inner<M: ManageConnection> {
    manager: M,
    idle: Vec<IdleEntry<M::Connection>>,
    active: u32,
    max_size: u32,
    max_lifetime: Option<Duration>,
}

pub struct Pool<M: ManageConnection> {
    inner: Mutex<Inner<M>>,
    notify: Condvar,
    connection_timeout: Duration,
}

pub struct PoolBuilder {
    max_size: u32,
    min_idle: u32,
    max_lifetime: Option<Duration>,
    connection_timeout: Duration,
}

impl Default for PoolBuilder {
    fn default() -> Self {
        Self {
            max_size: 10,
            min_idle: 0,
            max_lifetime: None,
            connection_timeout: Duration::from_secs(30),
        }
    }
}

impl PoolBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn max_size(mut self, n: u32) -> Self {
        self.max_size = n.max(1);
        self
    }

    pub fn min_idle(mut self, n: Option<u32>) -> Self {
        self.min_idle = n.unwrap_or(0);
        self
    }

    pub fn max_lifetime(mut self, d: Option<Duration>) -> Self {
        self.max_lifetime = d;
        self
    }

    pub fn connection_timeout(mut self, d: Duration) -> Self {
        self.connection_timeout = d;
        self
    }

    pub fn build<M: ManageConnection>(self, manager: M) -> Result<Arc<Pool<M>>, PoolError> {
        let pool = Arc::new(Pool {
            inner: Mutex::new(Inner {
                manager,
                idle: Vec::new(),
                active: 0,
                max_size: self.max_size,
                max_lifetime: self.max_lifetime,
            }),
            notify: Condvar::new(),
            connection_timeout: self.connection_timeout,
        });
        for _ in 0..self.min_idle {
            let conn = pool
                .inner
                .lock()
                .unwrap()
                .manager
                .connect()
                .map_err(PoolError::Connect)?;
            pool.inner.lock().unwrap().idle.push(IdleEntry {
                conn,
                created: Instant::now(),
            });
        }
        Ok(pool)
    }
}

impl<M: ManageConnection> Pool<M> {
    pub fn builder() -> PoolBuilder {
        PoolBuilder::new()
    }

    pub fn get(self: &Arc<Self>) -> Result<PooledConnection<M>, PoolError> {
        let deadline = Instant::now() + self.connection_timeout;
        let mut guard = self.inner.lock().unwrap();
        loop {
            while let Some(entry) = guard.idle.pop() {
                let expired = guard
                    .max_lifetime
                    .is_some_and(|life| entry.created.elapsed() > life);
                if expired {
                    continue;
                }
                if guard.manager.is_valid(&entry.conn).is_err() {
                    continue;
                }
                guard.active += 1;
                return Ok(PooledConnection {
                    conn: Some(entry.conn),
                    pool: Arc::clone(self),
                });
            }
            if guard.active < guard.max_size {
                let conn = guard.manager.connect().map_err(PoolError::Connect)?;
                guard.active += 1;
                return Ok(PooledConnection {
                    conn: Some(conn),
                    pool: Arc::clone(self),
                });
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(PoolError::Timeout);
            }
            let wait = deadline - now;
            let (g, _) = self.notify.wait_timeout(guard, wait).unwrap();
            guard = g;
        }
    }

    pub fn state(self: &Arc<Self>) -> PoolState {
        let guard = self.inner.lock().unwrap();
        PoolState {
            connections: guard.active + guard.idle.len() as u32,
            idle_connections: guard.idle.len() as u32,
        }
    }

    fn put_back(&self, conn: M::Connection) {
        let mut guard = self.inner.lock().unwrap();
        guard.active = guard.active.saturating_sub(1);
        guard.idle.push(IdleEntry {
            conn,
            created: Instant::now(),
        });
        self.notify.notify_one();
    }
}

pub struct PooledConnection<M: ManageConnection> {
    conn: Option<M::Connection>,
    pool: Arc<Pool<M>>,
}

impl<M: ManageConnection> Deref for PooledConnection<M> {
    type Target = M::Connection;
    fn deref(&self) -> &Self::Target {
        self.conn.as_ref().unwrap()
    }
}

impl<M: ManageConnection> DerefMut for PooledConnection<M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.conn.as_mut().unwrap()
    }
}

impl<M: ManageConnection> Drop for PooledConnection<M> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.put_back(conn);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct CounterManager {
        n: AtomicU32,
    }

    impl ManageConnection for CounterManager {
        type Connection = u32;
        fn connect(&self) -> Result<Self::Connection, String> {
            Ok(self.n.fetch_add(1, Ordering::Relaxed))
        }
    }

    #[test]
    fn pool_reuses_idle() {
        let pool = Pool::<CounterManager>::builder()
            .max_size(2)
            .build(CounterManager {
                n: AtomicU32::new(1),
            })
            .unwrap();
        {
            let _c1 = pool.get().unwrap();
            let _c2 = pool.get().unwrap();
        }
        let _c3 = pool.get().unwrap();
        assert_eq!(pool.state().connections, 2);
    }
}
