//! Auth header helpers.

use niao_codec::base64::encode_standard;

/// `Authorization: Basic …` header value (without the scheme prefix option).
pub fn basic_auth(user: &str, pass: &str) -> String {
    let cred = format!("{user}:{pass}");
    format!("Basic {}", encode_standard(cred.as_bytes()))
}

/// `Authorization: Bearer …` header value.
pub fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_and_bearer() {
        let b = basic_auth("aladdin", "opensesame");
        assert!(b.starts_with("Basic "));
        assert_eq!(bearer("tok"), "Bearer tok");
    }
}
