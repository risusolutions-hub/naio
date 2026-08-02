//! Password hashing / verify via `niao_pass` (argon2id default).

use crate::error::AuthResult;
use niao_pass::{verify_password, CryptContext, Scheme};

/// Hash a password with the given context (or default argon2id).
pub fn hash_with(ctx: &CryptContext, password: &str) -> AuthResult<String> {
    Ok(ctx.hash(password, None)?)
}

/// Hash with default CryptContext.
pub fn hash(password: &str) -> AuthResult<String> {
    hash_with(&CryptContext::default(), password)
}

/// Verify password against a stored hash (auto-detect scheme).
pub fn verify(password: &str, hash: &str) -> AuthResult<bool> {
    Ok(verify_password(password, hash)?)
}

/// Verify and optionally return a rehashed password if the scheme is deprecated.
pub fn verify_and_update(
    ctx: &CryptContext,
    password: &str,
    hash: &str,
) -> AuthResult<VerifyUpdate> {
    let r = ctx.verify_and_update(password, hash)?;
    let updated = r.valid && r.new_hash.is_some();
    Ok(VerifyUpdate {
        ok: r.valid,
        hash: r.new_hash,
        updated,
    })
}

#[derive(Debug, Clone)]
pub struct VerifyUpdate {
    pub ok: bool,
    pub hash: Option<String>,
    pub updated: bool,
}

/// Build a CryptContext from simple opts (scheme name + bcrypt cost + argon2 params).
pub fn context_from_opts(
    scheme: Option<&str>,
    bcrypt_cost: Option<u32>,
    memory_kib: Option<u32>,
    time_cost: Option<u32>,
) -> AuthResult<CryptContext> {
    let mut ctx = CryptContext::default();
    if let Some(name) = scheme {
        let s = Scheme::parse(name).map_err(|e| crate::error::AuthError::Password(e.message()))?;
        ctx.default_scheme = s;
        if !ctx.schemes.contains(&s) {
            ctx.schemes.push(s);
        }
    }
    if let Some(c) = bcrypt_cost {
        ctx.bcrypt_cost = c;
    }
    if memory_kib.is_some() || time_cost.is_some() {
        ctx.argon2 = niao_pass::Argon2Opts::from_map(memory_kib, time_cost, None)?;
    }
    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_verify_bcrypt_fast() {
        let ctx = context_from_opts(Some("bcrypt"), Some(4), None, None).unwrap();
        let h = hash_with(&ctx, "s3cret!").unwrap();
        assert!(verify("s3cret!", &h).unwrap());
        assert!(!verify("wrong", &h).unwrap());
    }
}
