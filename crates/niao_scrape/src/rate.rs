//! Per-host polite rate limiter.

use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

/// Simple delay-based limiter: at most one request per `delay_ms` per host.
#[derive(Debug, Clone)]
pub struct Limiter {
    pub delay_ms: u64,
    last: HashMap<String, Instant>,
    pub waits: u64,
    pub total_wait_ms: u64,
}

impl Default for Limiter {
    fn default() -> Self {
        Self {
            delay_ms: 500,
            last: HashMap::new(),
            waits: 0,
            total_wait_ms: 0,
        }
    }
}

impl Limiter {
    pub fn new(delay_ms: u64) -> Self {
        Self {
            delay_ms,
            ..Default::default()
        }
    }

    /// Block until the host slot is free. Returns milliseconds actually slept.
    pub fn wait(&mut self, host: &str) -> u64 {
        if self.delay_ms == 0 || host.is_empty() {
            return 0;
        }
        let key = host.to_ascii_lowercase();
        let now = Instant::now();
        if let Some(prev) = self.last.get(&key) {
            let elapsed = now.duration_since(*prev);
            let need = Duration::from_millis(self.delay_ms);
            if elapsed < need {
                let sleep_for = need - elapsed;
                let ms = sleep_for.as_millis() as u64;
                thread::sleep(sleep_for);
                self.waits += 1;
                self.total_wait_ms += ms;
                self.last.insert(key, Instant::now());
                return ms;
            }
        }
        self.last.insert(key, Instant::now());
        0
    }

    pub fn info(&self) -> LimiterInfo {
        LimiterInfo {
            delay_ms: self.delay_ms,
            hosts_tracked: self.last.len() as u64,
            waits: self.waits,
            total_wait_ms: self.total_wait_ms,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LimiterInfo {
    pub delay_ms: u64,
    pub hosts_tracked: u64,
    pub waits: u64,
    pub total_wait_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_wait_sleeps() {
        let mut lim = Limiter::new(30);
        assert_eq!(lim.wait("ex.com"), 0);
        let slept = lim.wait("ex.com");
        assert!(slept > 0);
        assert_eq!(lim.wait("other.com"), 0);
    }
}
