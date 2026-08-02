//! In-memory webhook-id replay guard (idempotency / replay defense).

use std::collections::{HashMap, VecDeque};

/// Sliding-window set of recently seen message IDs.
#[derive(Debug, Clone)]
pub struct ReplayGuard {
    /// Ordered insertion times (unix secs) + ids for eviction.
    queue: VecDeque<(i64, String)>,
    /// id -> last-seen unix secs
    seen: HashMap<String, i64>,
    /// Max age for remembered IDs (seconds).
    max_age: i64,
    /// Hard capacity cap (oldest evicted first).
    capacity: usize,
}

impl ReplayGuard {
    /// Create a guard. `max_age` defaults to 300s; `capacity` defaults to 10_000.
    pub fn new(max_age: i64, capacity: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            seen: HashMap::new(),
            max_age: if max_age <= 0 { 300 } else { max_age },
            capacity: if capacity == 0 { 10_000 } else { capacity },
        }
    }

    /// Evict expired / over-capacity entries relative to `now`.
    pub fn prune(&mut self, now: i64) {
        while let Some(&(ts, _)) = self.queue.front() {
            if now - ts > self.max_age {
                if let Some((_, id)) = self.queue.pop_front() {
                    if self.seen.get(&id).copied() == Some(ts) {
                        self.seen.remove(&id);
                    }
                }
            } else {
                break;
            }
        }
        while self.queue.len() > self.capacity {
            if let Some((ts, id)) = self.queue.pop_front() {
                if self.seen.get(&id).copied() == Some(ts) {
                    self.seen.remove(&id);
                }
            }
        }
    }

    /// Return `true` if `id` was already recorded (without updating).
    pub fn seen(&mut self, id: &str, now: i64) -> bool {
        self.prune(now);
        self.seen.contains_key(id)
    }

    /// Record `id` if new. Returns `true` if this is the first sighting,
    /// `false` if it is a replay of a known id.
    pub fn check(&mut self, id: &str, now: i64) -> bool {
        self.prune(now);
        if self.seen.contains_key(id) {
            return false;
        }
        self.seen.insert(id.to_string(), now);
        self.queue.push_back((now, id.to_string()));
        // Enforce capacity immediately.
        self.prune(now);
        true
    }

    /// Forget a single id.
    pub fn forget(&mut self, id: &str) {
        self.seen.remove(id);
        self.queue.retain(|(_, i)| i != id);
    }

    /// Clear all remembered ids.
    pub fn clear(&mut self) {
        self.seen.clear();
        self.queue.clear();
    }

    /// Number of currently remembered ids.
    pub fn size(&self) -> usize {
        self.seen.len()
    }

    pub fn max_age(&self) -> i64 {
        self.max_age
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_then_replay() {
        let mut g = ReplayGuard::new(300, 100);
        assert!(g.check("a", 1000));
        assert!(!g.check("a", 1001));
        assert!(g.seen("a", 1001));
        assert!(!g.seen("b", 1001));
        assert!(g.check("b", 1001));
    }

    #[test]
    fn expires() {
        let mut g = ReplayGuard::new(10, 100);
        assert!(g.check("a", 1000));
        assert!(!g.check("a", 1005));
        assert!(g.check("a", 1011)); // expired
    }

    #[test]
    fn capacity() {
        let mut g = ReplayGuard::new(10_000, 2);
        assert!(g.check("a", 1));
        assert!(g.check("b", 2));
        assert!(g.check("c", 3));
        assert_eq!(g.size(), 2);
        assert!(!g.seen("a", 3));
    }
}
