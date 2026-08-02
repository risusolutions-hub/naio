//! `niao_keyring` — OS credential stores (Keychain, Secret Service, Windows Credential Manager).
//!
//! Cross-platform credential storage via the Rust [`keyring`] crate (~Python `keyring` subset).

pub mod error;
pub mod store;

pub use error::{KeyringError, KeyringResult};
pub use store::{
    backend_mode, backend_name, clear_memory, delete_credential, exists, get_password, get_secret,
    platform_name, set_password, set_secret, use_memory, use_system, BackendMode,
};

/// Credential tuple returned by `get_credential`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    pub service: String,
    pub username: String,
    pub password: String,
}

/// Fetch password credential; returns `None` when missing.
pub fn get_credential(service: &str, username: &str) -> KeyringResult<Option<Credential>> {
    match get_password(service, username)? {
        Some(password) => Ok(Some(Credential {
            service: service.to_owned(),
            username: username.to_owned(),
            password,
        })),
        None => Ok(None),
    }
}

/// Update password in place, or create a new credential.
pub fn set_credential(service: &str, username: &str, password: &str) -> KeyringResult<()> {
    set_password(service, username, password)
}

/// Remove credential; errors when not found (~Python `delete_password`).
pub fn delete_password(service: &str, username: &str) -> KeyringResult<()> {
    delete_credential(service, username)
}

#[cfg(test)]
mod integration {
    use super::*;

    #[test]
    fn credential_helpers() {
        store::use_memory();
        store::clear_memory();
        set_credential("app", "alice", "pw").unwrap();
        let c = get_credential("app", "alice").unwrap().unwrap();
        assert_eq!(c.password, "pw");
        delete_password("app", "alice").unwrap();
        store::use_system();
        store::clear_memory();
    }
}
