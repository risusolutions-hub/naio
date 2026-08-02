//! bcrypt password hashing.

use crate::error::{check_password_len, PassError, PassResult};

pub const DEFAULT_COST: u32 = 12;
pub const MIN_COST: u32 = 4;
pub const MAX_COST: u32 = 31;

pub fn hash_password(password: &str, cost: u32) -> PassResult<String> {
    check_password_len(password)?;
    if !(MIN_COST..=MAX_COST).contains(&cost) {
        return Err(PassError::InvalidParameter(format!(
            "bcrypt cost must be {MIN_COST}..={MAX_COST}"
        )));
    }
    bcrypt::hash(password, cost).map_err(|e| PassError::HashFailed(e.to_string()))
}

pub fn verify_password(password: &str, encoded: &str) -> PassResult<bool> {
    check_password_len(password)?;
    match bcrypt::verify(password, encoded) {
        Ok(ok) => Ok(ok),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("Invalid") || msg.contains("invalid") {
                Err(PassError::InvalidHash(msg))
            } else {
                Ok(false)
            }
        }
    }
}

pub fn needs_update(encoded: &str, min_cost: u32) -> PassResult<bool> {
    let cost = parse_cost(encoded)?;
    Ok(cost < min_cost)
}

fn parse_cost(encoded: &str) -> PassResult<u32> {
    let parts: Vec<&str> = encoded.split('$').collect();
    if parts.len() < 4 {
        return Err(PassError::InvalidHash("malformed bcrypt hash".into()));
    }
    parts[2]
        .parse()
        .map_err(|_| PassError::InvalidHash("invalid bcrypt cost".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let h = hash_password("secret", 4).unwrap();
        assert!(h.starts_with("$2b$"));
        assert!(verify_password("secret", &h).unwrap());
        assert!(!verify_password("nope", &h).unwrap());
    }
}
