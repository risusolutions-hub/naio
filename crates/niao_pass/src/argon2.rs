//! Argon2id password hashing (PHC format).

use crate::error::{check_password_len, PassError, PassResult};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2, Params, Version,
};
use niao_rand::fill_os_random;

#[derive(Debug, Clone)]
pub struct Argon2Opts {
    pub memory_kib: u32,
    pub time_cost: u32,
    pub parallelism: u32,
}

impl Default for Argon2Opts {
    fn default() -> Self {
        Self {
            memory_kib: 19_456,
            time_cost: 2,
            parallelism: 1,
        }
    }
}

impl Argon2Opts {
    pub fn from_map(
        memory_kib: Option<u32>,
        time_cost: Option<u32>,
        parallelism: Option<u32>,
    ) -> PassResult<Self> {
        let mut opts = Self::default();
        if let Some(m) = memory_kib {
            if m < 8 {
                return Err(PassError::InvalidParameter(
                    "memory_kib must be >= 8".into(),
                ));
            }
            opts.memory_kib = m;
        }
        if let Some(t) = time_cost {
            if t == 0 {
                return Err(PassError::InvalidParameter("time_cost must be > 0".into()));
            }
            opts.time_cost = t;
        }
        if let Some(p) = parallelism {
            if p == 0 {
                return Err(PassError::InvalidParameter(
                    "parallelism must be > 0".into(),
                ));
            }
            opts.parallelism = p;
        }
        Ok(opts)
    }

    fn build(&self) -> PassResult<Argon2<'static>> {
        let params = Params::new(self.memory_kib, self.time_cost, self.parallelism, None)
            .map_err(|e| PassError::InvalidParameter(e.to_string()))?;
        Ok(Argon2::new(
            argon2::Algorithm::Argon2id,
            Version::V0x13,
            params,
        ))
    }
}

pub fn hash_password(password: &str, opts: &Argon2Opts) -> PassResult<String> {
    check_password_len(password)?;
    let argon2 = opts.build()?;
    let mut salt_bytes = [0u8; 16];
    fill_os_random(&mut salt_bytes);
    let salt =
        SaltString::encode_b64(&salt_bytes).map_err(|e| PassError::HashFailed(e.to_string()))?;
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| PassError::HashFailed(e.to_string()))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, encoded: &str) -> PassResult<bool> {
    check_password_len(password)?;
    let parsed = PasswordHash::new(encoded).map_err(|e| PassError::InvalidHash(e.to_string()))?;
    let argon2 = Argon2::default();
    Ok(argon2.verify_password(password.as_bytes(), &parsed).is_ok())
}

pub fn needs_update(encoded: &str, opts: &Argon2Opts) -> PassResult<bool> {
    let parsed = PasswordHash::new(encoded).map_err(|e| PassError::InvalidHash(e.to_string()))?;
    let m = parsed.params.get_decimal("m").unwrap_or(0);
    let t = parsed.params.get_decimal("t").unwrap_or(0);
    let p = parsed.params.get_decimal("p").unwrap_or(0);
    Ok(m < opts.memory_kib || t < opts.time_cost || p < opts.parallelism)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let opts = Argon2Opts {
            memory_kib: 8_192,
            time_cost: 1,
            parallelism: 1,
        };
        let h = hash_password("hunter2", &opts).unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("hunter2", &h).unwrap());
        assert!(!verify_password("wrong", &h).unwrap());
    }
}
