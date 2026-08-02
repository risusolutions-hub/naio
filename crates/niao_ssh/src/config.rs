//! Connection configuration.

/// Options for [`crate::connect`].
#[derive(Debug, Clone)]
pub struct ConnectConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub key_path: Option<String>,
    pub key_data: Option<String>,
    pub passphrase: Option<String>,
    pub agent: bool,
    pub timeout_ms: Option<u64>,
}

impl ConnectConfig {
    pub fn new(host: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: 22,
            user: user.into(),
            password: None,
            key_path: None,
            key_data: None,
            passphrase: None,
            agent: false,
            timeout_ms: None,
        }
    }
}
