//! In-memory and system credential store routing.

use crate::error::{KeyringError, KeyringResult};
use keyring::Entry;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendMode {
    System,
    Memory,
}

static MEMORY: OnceLock<RwLock<HashMap<(String, String), Vec<u8>>>> = OnceLock::new();

thread_local! {
    static MODE: std::cell::Cell<BackendMode> = const { std::cell::Cell::new(BackendMode::System) };
}

fn memory() -> &'static RwLock<HashMap<(String, String), Vec<u8>>> {
    MEMORY.get_or_init(|| RwLock::new(HashMap::new()))
}

fn key(service: &str, user: &str) -> (String, String) {
    (service.to_owned(), user.to_owned())
}

fn open_entry(service: &str, user: &str) -> KeyringResult<Entry> {
    Entry::new(service, user).map_err(KeyringError::from)
}

/// Active backend mode for the current thread.
pub fn backend_mode() -> BackendMode {
    MODE.with(|m| m.get())
}

/// Route subsequent operations on this thread to an in-memory store (tests).
pub fn use_memory() {
    MODE.with(|m| m.set(BackendMode::Memory));
}

/// Restore the OS credential store for this thread.
pub fn use_system() {
    MODE.with(|m| m.set(BackendMode::System));
}

/// Clear all entries in the global in-memory store.
pub fn clear_memory() {
    memory().write().clear();
}

pub fn set_password(service: &str, user: &str, password: &str) -> KeyringResult<()> {
    if password.is_empty() {
        return Err(KeyringError::Invalid("password must not be empty".into()));
    }
    match backend_mode() {
        BackendMode::Memory => {
            memory()
                .write()
                .insert(key(service, user), password.as_bytes().to_vec());
            Ok(())
        }
        BackendMode::System => open_entry(service, user)?
            .set_password(password)
            .map_err(KeyringError::from),
    }
}

pub fn get_password(service: &str, user: &str) -> KeyringResult<Option<String>> {
    match backend_mode() {
        BackendMode::Memory => Ok(memory()
            .read()
            .get(&key(service, user))
            .map(|b| String::from_utf8_lossy(b).into_owned())),
        BackendMode::System => match open_entry(service, user)?
            .get_password()
            .map_err(KeyringError::from)
        {
            Ok(v) => Ok(Some(v)),
            Err(KeyringError::NotFound) => Ok(None),
            Err(e) => Err(e),
        },
    }
}

pub fn set_secret(service: &str, user: &str, secret: &[u8]) -> KeyringResult<()> {
    if secret.is_empty() {
        return Err(KeyringError::Invalid("secret must not be empty".into()));
    }
    match backend_mode() {
        BackendMode::Memory => {
            memory().write().insert(key(service, user), secret.to_vec());
            Ok(())
        }
        BackendMode::System => open_entry(service, user)?
            .set_secret(secret)
            .map_err(KeyringError::from),
    }
}

pub fn get_secret(service: &str, user: &str) -> KeyringResult<Option<Vec<u8>>> {
    match backend_mode() {
        BackendMode::Memory => Ok(memory().read().get(&key(service, user)).cloned()),
        BackendMode::System => match open_entry(service, user)?
            .get_secret()
            .map_err(KeyringError::from)
        {
            Ok(v) => Ok(Some(v)),
            Err(KeyringError::NotFound) => Ok(None),
            Err(e) => Err(e),
        },
    }
}

pub fn delete_credential(service: &str, user: &str) -> KeyringResult<()> {
    match backend_mode() {
        BackendMode::Memory => {
            if memory().write().remove(&key(service, user)).is_some() {
                Ok(())
            } else {
                Err(KeyringError::NotFound)
            }
        }
        BackendMode::System => open_entry(service, user)?
            .delete_credential()
            .map_err(KeyringError::from),
    }
}

pub fn exists(service: &str, user: &str) -> KeyringResult<bool> {
    Ok(get_password(service, user)?.is_some() || get_secret(service, user)?.is_some())
}

pub fn platform_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    }
}

pub fn backend_name() -> String {
    match backend_mode() {
        BackendMode::Memory => "memory".into(),
        BackendMode::System => match platform_name() {
            "macos" => "keychain".into(),
            "windows" => "windows_credential_manager".into(),
            "linux" => "secret_service".into(),
            other => other.into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_scope() -> Guard {
        use_memory();
        clear_memory();
        Guard
    }

    struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            use_system();
            clear_memory();
        }
    }

    #[test]
    fn memory_password_roundtrip() {
        let _g = mem_scope();
        set_password("svc", "user", "secret").unwrap();
        assert_eq!(get_password("svc", "user").unwrap(), Some("secret".into()));
        assert!(exists("svc", "user").unwrap());
        delete_credential("svc", "user").unwrap();
        assert_eq!(get_password("svc", "user").unwrap(), None);
    }

    #[test]
    fn memory_secret_roundtrip() {
        let _g = mem_scope();
        set_secret("svc", "user", &[1, 2, 3]).unwrap();
        assert_eq!(get_secret("svc", "user").unwrap(), Some(vec![1, 2, 3]));
    }

    #[test]
    fn delete_missing_errors() {
        let _g = mem_scope();
        assert!(matches!(
            delete_credential("nope", "nope"),
            Err(KeyringError::NotFound)
        ));
    }
}
