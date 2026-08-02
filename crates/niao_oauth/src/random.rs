use niao_codec::base64::encode_url_safe_no_pad;
use niao_rand::{thread_rng, Rng};

const STATE_BYTES: usize = 32;
const NONCE_BYTES: usize = 32;
const PKCE_VERIFIER_LEN: usize = 64;

/// URL-safe random string suitable for OAuth `state`.
///
/// >>> use niao_oauth::random_state;
/// >>> let s = random_state();
/// >>> s.len() >= 32
/// true
pub fn random_state() -> String {
    random_url_safe(STATE_BYTES)
}

/// URL-safe random string suitable for OIDC `nonce`.
///
/// >>> use niao_oauth::random_nonce;
/// >>> random_nonce().len() >= 32
/// true
pub fn random_nonce() -> String {
    random_url_safe(NONCE_BYTES)
}

/// Generate a PKCE code verifier (RFC 7636).
///
/// >>> use niao_oauth::random_verifier;
/// >>> (43..=128).contains(&random_verifier().len())
/// true
pub fn random_verifier() -> String {
    let mut rng = thread_rng();
    let mut bytes = vec![0u8; PKCE_VERIFIER_LEN];
    for b in &mut bytes {
        *b = (rng.next_u64() & 0xff) as u8;
    }
    // RFC 7636: unreserved chars only
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    bytes
        .into_iter()
        .map(|b| ALPHABET[(b as usize) % ALPHABET.len()] as char)
        .collect()
}

fn random_url_safe(n: usize) -> String {
    let mut rng = thread_rng();
    let mut bytes = vec![0u8; n];
    for b in &mut bytes {
        *b = (rng.next_u64() & 0xff) as u8;
    }
    encode_url_safe_no_pad(&bytes)
}
