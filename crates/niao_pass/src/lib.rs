//! Password hashing: argon2id, bcrypt, scrypt + strength policy checks.
//!
//! ~passlib, argon2-cffi, bcrypt subset.

pub mod argon2;
pub mod bcrypt;
pub mod scrypt;

mod common;
mod context;
mod error;
mod generate;
mod policy;
mod scheme;

pub use argon2::Argon2Opts;
pub use bcrypt::{DEFAULT_COST, MAX_COST, MIN_COST};
pub use common::is_common_password;
pub use context::{
    hash_password, needs_update_hash, verify_password, CryptContext, VerifyUpdateResult,
};
pub use error::{check_password_len, PassError, PassResult, MAX_PASSWORD_BYTES};
pub use generate::{generate, generate_bytes, DEFAULT_ALPHABET};
pub use policy::{
    check_strength, estimate_entropy, score_password, CharClasses, Policy, StrengthReport,
};
pub use scheme::{identify, Scheme};
pub use scrypt::ScryptOpts;

pub const DEFAULT_SCHEME: Scheme = Scheme::Argon2id;

#[cfg(test)]
mod integration {
    use super::*;

    #[test]
    fn context_roundtrip_all_schemes() {
        let ctx = CryptContext::default();
        for scheme in Scheme::ALL {
            let h = ctx.hash("S3cret!", Some(*scheme)).unwrap();
            assert!(ctx.verify("S3cret!", &h).unwrap());
            assert_eq!(identify(&h), Some(*scheme));
        }
    }
}
