//! `niao_event` — in-process pub/sub engine with dot-separated topics and
//! `*` / `**` wildcard patterns. Used by the Niao `nevent` standard library.

mod topic;

pub use topic::{
    is_valid_pattern, is_valid_topic, join_topic, normalize, split_topic, topic_matches,
    TopicError, TopicPattern,
};

/// Subscription id issued by [`Emitter::subscribe`].
pub type SubId = u64;

/// Listener cap: `0` = unlimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmitterOptions {
    pub max_listeners_per_pattern: usize,
}

impl Default for EmitterOptions {
    fn default() -> Self {
        Self {
            max_listeners_per_pattern: 128,
        }
    }
}

/// Aggregate counters (cheap atomics not needed — single-threaded ownership).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmitterStats {
    pub emit_count: u64,
    pub delivery_count: u64,
    pub subscription_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscribeError {
    InvalidPattern(TopicError),
    MaxListeners,
}

impl std::fmt::Display for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubscribeError::InvalidPattern(e) => write!(f, "{e}"),
            SubscribeError::MaxListeners => write!(f, "max listeners exceeded for pattern"),
        }
    }
}

impl std::error::Error for SubscribeError {}

#[derive(Debug, Clone)]
struct Subscription {
    id: SubId,
    pattern: TopicPattern,
    once: bool,
    active: bool,
}

/// In-process event bus backing store (handlers live in the runtime).
#[derive(Debug)]
pub struct Emitter {
    opts: EmitterOptions,
    next_id: SubId,
    subs: Vec<Subscription>,
    /// Exact literal patterns → indices into `subs` (registration order preserved).
    exact: std::collections::HashMap<String, Vec<usize>>,
    /// Indices of wildcard patterns in `subs`.
    pattern_idx: Vec<usize>,
    paused: bool,
    stats: EmitterStats,
}

impl Default for Emitter {
    fn default() -> Self {
        Self::new(EmitterOptions::default())
    }
}

impl Emitter {
    pub fn new(opts: EmitterOptions) -> Self {
        Self {
            opts,
            next_id: 1,
            subs: Vec::new(),
            exact: std::collections::HashMap::new(),
            pattern_idx: Vec::new(),
            paused: false,
            stats: EmitterStats::default(),
        }
    }

    #[inline]
    pub fn options(&self) -> EmitterOptions {
        self.opts
    }

    #[inline]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn stats(&self) -> EmitterStats {
        let mut s = self.stats;
        s.subscription_count = self.subs.iter().filter(|s| s.active).count();
        s
    }

    /// Register a listener pattern. Set `once` for single-shot delivery.
    pub fn subscribe(&mut self, pattern: &str, once: bool) -> Result<SubId, SubscribeError> {
        let parsed = TopicPattern::parse(pattern).map_err(SubscribeError::InvalidPattern)?;
        if self.opts.max_listeners_per_pattern > 0 {
            let count = self
                .subs
                .iter()
                .filter(|s| s.active && s.pattern.as_str() == parsed.as_str())
                .count();
            if count >= self.opts.max_listeners_per_pattern {
                return Err(SubscribeError::MaxListeners);
            }
        }
        let id = self.next_id;
        self.next_id += 1;
        let idx = self.subs.len();
        let key = parsed.as_str().to_string();
        if parsed.has_wildcard() {
            self.pattern_idx.push(idx);
        } else {
            self.exact.entry(key).or_default().push(idx);
        }
        self.subs.push(Subscription {
            id,
            pattern: parsed,
            once,
            active: true,
        });
        Ok(id)
    }

    /// Remove subscription by id.
    pub fn unsubscribe_id(&mut self, id: SubId) -> bool {
        if let Some(sub) = self.subs.iter_mut().find(|s| s.id == id && s.active) {
            sub.active = false;
            return true;
        }
        false
    }

    /// Remove all subscriptions matching `pattern` (exact pattern string after normalize).
    pub fn unsubscribe_pattern(&mut self, pattern: &str) -> usize {
        let Ok(parsed) = TopicPattern::parse(pattern) else {
            return 0;
        };
        let key = parsed.as_str();
        let mut removed = 0;
        for sub in &mut self.subs {
            if sub.active && sub.pattern.as_str() == key {
                sub.active = false;
                removed += 1;
            }
        }
        removed
    }

    /// Remove every active subscription.
    pub fn clear(&mut self) -> usize {
        let mut n = 0;
        for sub in &mut self.subs {
            if sub.active {
                sub.active = false;
                n += 1;
            }
        }
        n
    }

    /// Collect active subscription ids matching `topic`, in registration order.
    pub fn matching_ids(&self, topic: &str) -> Vec<SubId> {
        let normalized = normalize(topic);
        if normalized.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        if let Some(indices) = self.exact.get(&normalized) {
            for &i in indices {
                let sub = &self.subs[i];
                if sub.active {
                    out.push(sub.id);
                }
            }
        }
        for &i in &self.pattern_idx {
            let sub = &self.subs[i];
            if sub.active && sub.pattern.matches(&normalized) {
                out.push(sub.id);
            }
        }
        out
    }

    /// Lookup pattern string for a subscription id.
    pub fn pattern_for(&self, id: SubId) -> Option<&str> {
        self.subs
            .iter()
            .find(|s| s.id == id && s.active)
            .map(|s| s.pattern.as_str())
    }

    /// Whether `id` is a once subscription.
    pub fn is_once(&self, id: SubId) -> bool {
        self.subs
            .iter()
            .find(|s| s.id == id && s.active)
            .map(|s| s.once)
            .unwrap_or(false)
    }

    /// Mark once subscriptions consumed after delivery.
    pub fn consume_once(&mut self, ids: &[SubId]) {
        for id in ids {
            if let Some(sub) = self.subs.iter_mut().find(|s| s.id == *id) {
                if sub.once {
                    sub.active = false;
                }
            }
        }
    }

    /// Count active subscriptions optionally filtered by topic match.
    pub fn listener_count(&self, topic_filter: Option<&str>) -> usize {
        let Some(filter) = topic_filter else {
            return self.subs.iter().filter(|s| s.active).count();
        };
        let normalized = normalize(filter);
        if normalized.is_empty() {
            return 0;
        }
        self.subs
            .iter()
            .filter(|s| s.active && s.pattern.matches(&normalized))
            .count()
    }

    /// Distinct active pattern strings.
    pub fn topics(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for sub in &self.subs {
            if sub.active {
                let key = sub.pattern.as_str().to_string();
                if seen.insert(key.clone()) {
                    out.push(key);
                }
            }
        }
        out
    }

    pub fn has_listeners(&self, topic_filter: Option<&str>) -> bool {
        self.listener_count(topic_filter) > 0
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Record one emit (called by runtime after dispatch).
    pub fn record_emit(&mut self, deliveries: usize) {
        self.stats.emit_count += 1;
        self.stats.delivery_count += deliveries as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribe_and_match() {
        let mut e = Emitter::default();
        let a = e.subscribe("user.created", false).unwrap();
        let b = e.subscribe("user.*", false).unwrap();
        let ids = e.matching_ids("user.created");
        assert_eq!(ids, vec![a, b]);
        e.consume_once(&[a]);
        assert!(e.matching_ids("user.created").contains(&b));
        let _ = a;
    }

    #[test]
    fn once_removed_after_consume() {
        let mut e = Emitter::default();
        let id = e.subscribe("x", true).unwrap();
        e.consume_once(&[id]);
        assert!(e.matching_ids("x").is_empty());
    }

    #[test]
    fn max_listeners_enforced() {
        let mut e = Emitter::new(EmitterOptions {
            max_listeners_per_pattern: 2,
        });
        assert!(e.subscribe("a", false).is_ok());
        assert!(e.subscribe("a", false).is_ok());
        assert!(matches!(
            e.subscribe("a", false),
            Err(SubscribeError::MaxListeners)
        ));
    }
}
