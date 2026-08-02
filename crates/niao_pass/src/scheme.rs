//! Hash scheme identification and parsing.

use crate::error::{PassError, PassResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scheme {
    Argon2id,
    Bcrypt,
    Scrypt,
}

impl Scheme {
    pub const ALL: &'static [Scheme] = &[Scheme::Argon2id, Scheme::Bcrypt, Scheme::Scrypt];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Argon2id => "argon2id",
            Self::Bcrypt => "bcrypt",
            Self::Scrypt => "scrypt",
        }
    }

    pub fn parse(name: &str) -> PassResult<Self> {
        match name.to_ascii_lowercase().as_str() {
            "argon2id" | "argon2" => Ok(Self::Argon2id),
            "bcrypt" | "bcrypt_sha256" | "2b" => Ok(Self::Bcrypt),
            "scrypt" => Ok(Self::Scrypt),
            other => Err(PassError::UnknownScheme(other.to_string())),
        }
    }
}

/// Identify the hashing scheme from an encoded hash string.
pub fn identify(hash: &str) -> Option<Scheme> {
    if hash.starts_with("$argon2id$")
        || hash.starts_with("$argon2i$")
        || hash.starts_with("$argon2d$")
    {
        Some(Scheme::Argon2id)
    } else if hash.starts_with("$2a$")
        || hash.starts_with("$2b$")
        || hash.starts_with("$2y$")
        || hash.starts_with("$2x$")
    {
        Some(Scheme::Bcrypt)
    } else if hash.starts_with("$scrypt$") {
        Some(Scheme::Scrypt)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identify_bcrypt_prefix() {
        assert_eq!(
            identify("$2b$12$abcdefghijklmnopqrstuu"),
            Some(Scheme::Bcrypt)
        );
    }

    #[test]
    fn identify_unknown() {
        assert_eq!(identify("not-a-hash"), None);
    }
}
