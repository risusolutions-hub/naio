//! Timer min-heap for executor sleep/poll timeouts.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::{Duration, Instant};

struct TimerEntry {
    deadline: Instant,
    id: u64,
}

impl PartialEq for TimerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline
    }
}
impl Eq for TimerEntry {}

impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other.deadline.cmp(&self.deadline)
    }
}

pub struct TimerQueue {
    heap: BinaryHeap<TimerEntry>,
    next_id: u64,
}

impl TimerQueue {
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            next_id: 1,
        }
    }

    pub fn schedule(&mut self, after: Duration) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.heap.push(TimerEntry {
            deadline: Instant::now() + after,
            id,
        });
        id
    }

    pub fn poll_timeout_ms(&self) -> Option<u32> {
        self.heap.peek().map(|top| {
            let now = Instant::now();
            if top.deadline <= now {
                0
            } else {
                (top.deadline - now).as_millis().min(u32::MAX as u128) as u32
            }
        })
    }

    pub fn pop_expired(&mut self, now: Instant) -> Vec<u64> {
        let mut out = Vec::new();
        while self.heap.peek().is_some_and(|t| t.deadline <= now) {
            out.push(self.heap.pop().unwrap().id);
        }
        out
    }
}

pub fn sleep(dur: Duration) {
    std::thread::sleep(dur);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_fires_in_order() {
        let mut q = TimerQueue::new();
        q.schedule(Duration::from_millis(30));
        q.schedule(Duration::from_millis(5));
        std::thread::sleep(Duration::from_millis(10));
        let fired = q.pop_expired(Instant::now());
        assert_eq!(fired.len(), 1);
    }
}
