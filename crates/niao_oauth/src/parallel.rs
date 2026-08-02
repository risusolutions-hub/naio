use crate::client::OAuthClient;
use crate::error::OAuthResult;
use crate::token::{
    client_credentials, refresh_token, ClientCredentialsOptions, RefreshOptions, TokenResponse,
};
use niao_parallel::available_threads;

#[derive(Debug, Clone, Copy)]
pub struct ParallelOpts {
    pub threads: usize,
}

impl Default for ParallelOpts {
    fn default() -> Self {
        Self {
            threads: available_threads(),
        }
    }
}

/// Refresh many tokens in parallel (rayon-style over niao_parallel).
pub fn parallel_refresh(
    clients: &[OAuthClient],
    refresh_tokens: &[String],
    opts: &RefreshOptions,
    par: &ParallelOpts,
) -> Vec<OAuthResult<TokenResponse>> {
    debug_assert_eq!(clients.len(), refresh_tokens.len());
    let n = clients.len();
    if n == 0 {
        return Vec::new();
    }
    let threads = par.threads.max(1).min(n);
    if threads == 1 || n == 1 {
        return (0..n)
            .map(|i| refresh_token(&clients[i], &refresh_tokens[i], opts))
            .collect();
    }
    let chunk = (n + threads - 1) / threads;
    let mut out = vec![Err(crate::error::OAuthError::Token("skipped".into())); n];
    std::thread::scope(|s| {
        for (t, slot) in out.chunks_mut(chunk).enumerate() {
            let start = t * chunk;
            let end = (start + slot.len()).min(n);
            s.spawn(move || {
                for i in start..end {
                    slot[i - start] = refresh_token(&clients[i], &refresh_tokens[i], opts);
                }
            });
        }
    });
    out
}

/// Client-credentials token fetch for many clients in parallel.
pub fn parallel_client_credentials(
    clients: &[OAuthClient],
    opts: &ClientCredentialsOptions,
    par: &ParallelOpts,
) -> Vec<OAuthResult<TokenResponse>> {
    let n = clients.len();
    if n == 0 {
        return Vec::new();
    }
    let threads = par.threads.max(1).min(n);
    if threads == 1 || n == 1 {
        return clients
            .iter()
            .map(|c| client_credentials(c, opts))
            .collect();
    }
    let chunk = (n + threads - 1) / threads;
    let mut out = vec![Err(crate::error::OAuthError::Token("skipped".into())); n];
    std::thread::scope(|s| {
        for (t, slot) in out.chunks_mut(chunk).enumerate() {
            let start = t * chunk;
            let end = (start + slot.len()).min(n);
            s.spawn(move || {
                for i in start..end {
                    slot[i - start] = client_credentials(&clients[i], opts);
                }
            });
        }
    });
    out
}
