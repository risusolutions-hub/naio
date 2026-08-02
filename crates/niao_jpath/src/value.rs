//! Shared JSON value helpers.

/// Compare two JSON values for structural equality (RFC 6901 test semantics).
pub fn values_equal(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_values() {
        let a = serde_json::json!({"x": [1, 2]});
        let b = serde_json::json!({"x": [1, 2]});
        assert!(values_equal(&a, &b));
    }
}
