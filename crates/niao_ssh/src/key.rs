//! Key loading and fingerprints.

use crate::error::{SshError, SshResult};
use russh::keys::{decode_secret_key, load_secret_key, HashAlg, PrivateKey, PublicKey};
use std::path::Path;
use std::sync::Arc;

/// Load a private key from a filesystem path.
pub fn load_key_file(path: &str, passphrase: Option<&str>) -> SshResult<PrivateKey> {
    load_secret_key(Path::new(path), passphrase).map_err(SshError::from)
}

/// Load a private key from OpenSSH / PEM text.
pub fn load_key_data(data: &str, passphrase: Option<&str>) -> SshResult<PrivateKey> {
    decode_secret_key(data, passphrase).map_err(SshError::from)
}

/// SHA256 fingerprint (`SHA256:…`) of a private key file or PEM blob.
pub fn key_fingerprint(
    path_or_pem: &str,
    is_path: bool,
    passphrase: Option<&str>,
) -> SshResult<String> {
    let key = if is_path {
        load_key_file(path_or_pem, passphrase)?
    } else {
        load_key_data(path_or_pem, passphrase)?
    };
    Ok(fingerprint_public(key.public_key()))
}

pub(crate) fn fingerprint_public(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

pub(crate) fn arc_key(key: PrivateKey) -> Arc<PrivateKey> {
    Arc::new(key)
}
