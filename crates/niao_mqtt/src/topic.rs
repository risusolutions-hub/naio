//! MQTT topic filter matching (`+` and `#` wildcards).

/// Return true if `topic` matches MQTT `filter` (subscription filter semantics).
pub fn topic_matches(filter: &str, topic: &str) -> bool {
    if filter.is_empty() || topic.is_empty() {
        return false;
    }
    // System topics: $ filters only match $ topics and vice versa (MQTT-4.7.2-1).
    let filter_sys = filter.starts_with('$');
    let topic_sys = topic.starts_with('$');
    if filter_sys != topic_sys {
        // Exception: filter "#" or "+/..." still shouldn't match system topics when filter doesn't start with $
        if !filter_sys && topic_sys {
            return false;
        }
    }

    let mut fparts = filter.split('/');
    let mut tparts = topic.split('/');

    loop {
        match (fparts.next(), tparts.next()) {
            (None, None) => return true,
            (Some("#"), _) => return true,
            (Some("+"), Some(_)) => continue,
            (Some(f), Some(t)) if f == t => continue,
            (Some(_), Some(_)) => return false,
            (None, Some(_)) => return false,
            (Some(_), None) => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_wildcards() {
        assert!(topic_matches("a/b", "a/b"));
        assert!(!topic_matches("a/b", "a/c"));
        assert!(topic_matches("a/+/c", "a/b/c"));
        assert!(topic_matches("a/#", "a/b/c"));
        assert!(topic_matches("#", "a/b"));
        assert!(!topic_matches("+", "a/b"));
        assert!(!topic_matches("sport/#", "$SYS/foo"));
        assert!(topic_matches("$SYS/#", "$SYS/foo"));
    }
}
