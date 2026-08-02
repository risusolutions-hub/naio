//! scrypt password hashing (PHC format).

use crate::error::{check_password_len, PassError, PassResult};
use password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use scrypt::{Params, Scrypt, ALG_ID};

const KEY_LEN: usize = 32;

#[derive(Debug, Clone)]
pub struct ScryptOpts {
    pub log_n: u8,
    pub r: u32,
    pub p: u32,
}

impl Default for ScryptOpts {
    fn default() -> Self {
        Self {
            log_n: 15,
            r: 8,
            p: 1,
        }
    }
}

impl ScryptOpts {
    pub fn from_map(log_n: Option<u8>, r: Option<u32>, p: Option<u32>) -> PassResult<Self> {
        let mut opts = Self::default();
        if let Some(n) = log_n {
            if n < 1 || n > 20 {
                return Err(PassError::InvalidParameter("log_n must be 1..=20".into()));
            }
            opts.log_n = n;
        }
        if let Some(r) = r {
            if r == 0 {
                return Err(PassError::InvalidParameter("r must be > 0".into()));
            }
            opts.r = r;
        }
        if let Some(p) = p {
            if p == 0 {
                return Err(PassError::InvalidParameter("p must be > 0".into()));
            }
            opts.p = p;
        }
        Ok(opts)
    }

    fn build(&self) -> PassResult<Params> {
        Params::new(self.log_n, self.r, self.p, KEY_LEN)
            .map_err(|_| PassError::InvalidParameter("invalid scrypt parameters".into()))
    }
}

pub fn hash_password(password: &str, opts: &ScryptOpts) -> PassResult<String> {
    check_password_len(password)?;
    let params = opts.build()?;
    let mut salt_bytes = [0u8; 16];
    niao_rand::fill_os_random(&mut salt_bytes);
    let salt =
        SaltString::encode_b64(&salt_bytes).map_err(|e| PassError::HashFailed(e.to_string()))?;
    let hash = Scrypt
        .hash_password_customized(password.as_bytes(), Some(ALG_ID), None, params, &salt)
        .map_err(|e| PassError::HashFailed(e.to_string()))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, encoded: &str) -> PassResult<bool> {
    check_password_len(password)?;
    let parsed = PasswordHash::new(encoded).map_err(|e| PassError::InvalidHash(e.to_string()))?;
    Ok(Scrypt.verify_password(password.as_bytes(), &parsed).is_ok())
}

pub fn needs_update(encoded: &str, opts: &ScryptOpts) -> PassResult<bool> {
    let parsed = PasswordHash::new(encoded).map_err(|e| PassError::InvalidHash(e.to_string()))?;
    let params = Params::try_from(&parsed).map_err(|e| PassError::InvalidHash(e.to_string()))?;
    Ok(params.log_n() < opts.log_n || params.r() < opts.r || params.p() < opts.p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let opts = ScryptOpts {
            log_n: 10,
            r: 8,
            p: 1,
        };
        let h = hash_password("secret", &opts).unwrap();
        assert!(h.starts_with("$scrypt$"));
        assert!(verify_password("secret", &h).unwrap());
        assert!(!verify_password("wrong", &h).unwrap());
    }
}
