//! Multi-scheme password context (~passlib CryptContext).

use crate::argon2::{self, Argon2Opts};
use crate::bcrypt::{self, DEFAULT_COST};
use crate::error::{PassError, PassResult};
use crate::scheme::{identify, Scheme};
use crate::scrypt::{self, ScryptOpts};

#[derive(Debug, Clone)]
pub struct VerifyUpdateResult {
    pub valid: bool,
    pub new_hash: Option<String>,
    pub scheme: Option<Scheme>,
}

#[derive(Debug, Clone)]
pub struct CryptContext {
    pub default_scheme: Scheme,
    pub schemes: Vec<Scheme>,
    pub deprecated: Vec<Scheme>,
    pub argon2: Argon2Opts,
    pub bcrypt_cost: u32,
    pub scrypt: ScryptOpts,
}

impl Default for CryptContext {
    fn default() -> Self {
        Self {
            default_scheme: Scheme::Argon2id,
            schemes: vec![Scheme::Argon2id, Scheme::Bcrypt, Scheme::Scrypt],
            deprecated: vec![Scheme::Bcrypt],
            argon2: Argon2Opts::default(),
            bcrypt_cost: DEFAULT_COST,
            scrypt: ScryptOpts::default(),
        }
    }
}

impl CryptContext {
    pub fn hash(&self, password: &str, scheme: Option<Scheme>) -> PassResult<String> {
        let scheme = scheme.unwrap_or(self.default_scheme);
        if !self.schemes.contains(&scheme) {
            return Err(PassError::UnsupportedScheme(scheme.as_str().into()));
        }
        self.hash_with_scheme(password, scheme)
    }

    pub fn hash_with_scheme(&self, password: &str, scheme: Scheme) -> PassResult<String> {
        match scheme {
            Scheme::Argon2id => argon2::hash_password(password, &self.argon2),
            Scheme::Bcrypt => bcrypt::hash_password(password, self.bcrypt_cost),
            Scheme::Scrypt => scrypt::hash_password(password, &self.scrypt),
        }
    }

    pub fn verify(&self, password: &str, encoded: &str) -> PassResult<bool> {
        let scheme = identify(encoded)
            .ok_or_else(|| PassError::InvalidHash("cannot identify hash scheme".into()))?;
        if !self.schemes.contains(&scheme) {
            return Err(PassError::UnsupportedScheme(scheme.as_str().into()));
        }
        match scheme {
            Scheme::Argon2id => argon2::verify_password(password, encoded),
            Scheme::Bcrypt => bcrypt::verify_password(password, encoded),
            Scheme::Scrypt => scrypt::verify_password(password, encoded),
        }
    }

    pub fn needs_update(&self, encoded: &str) -> PassResult<bool> {
        let scheme = identify(encoded)
            .ok_or_else(|| PassError::InvalidHash("cannot identify hash scheme".into()))?;
        if self.deprecated.contains(&scheme) {
            return Ok(true);
        }
        match scheme {
            Scheme::Argon2id => argon2::needs_update(encoded, &self.argon2),
            Scheme::Bcrypt => bcrypt::needs_update(encoded, self.bcrypt_cost),
            Scheme::Scrypt => scrypt::needs_update(encoded, &self.scrypt),
        }
    }

    pub fn verify_and_update(
        &self,
        password: &str,
        encoded: &str,
    ) -> PassResult<VerifyUpdateResult> {
        let scheme = identify(encoded);
        let valid = self.verify(password, encoded)?;
        if !valid {
            return Ok(VerifyUpdateResult {
                valid: false,
                new_hash: None,
                scheme,
            });
        }
        let update = self.needs_update(encoded)?;
        let new_hash = if update {
            Some(self.hash(password, None)?)
        } else {
            None
        };
        Ok(VerifyUpdateResult {
            valid: true,
            new_hash,
            scheme,
        })
    }
}

pub fn hash_password(password: &str, scheme: Scheme, ctx: &CryptContext) -> PassResult<String> {
    ctx.hash_with_scheme(password, scheme)
}

pub fn verify_password(password: &str, encoded: &str) -> PassResult<bool> {
    let scheme = identify(encoded)
        .ok_or_else(|| PassError::InvalidHash("cannot identify hash scheme".into()))?;
    match scheme {
        Scheme::Argon2id => argon2::verify_password(password, encoded),
        Scheme::Bcrypt => bcrypt::verify_password(password, encoded),
        Scheme::Scrypt => scrypt::verify_password(password, encoded),
    }
}

pub fn needs_update_hash(encoded: &str, ctx: &CryptContext) -> PassResult<bool> {
    ctx.needs_update(encoded)
}
