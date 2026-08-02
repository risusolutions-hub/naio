use niao_crypto::constant_time_eq;

/// Constant-time digest comparison (`secrets.compare_digest`).
pub fn compare_digest(a: &[u8], b: &[u8]) -> bool {
    constant_time_eq(a, b)
}
