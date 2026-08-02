//! OAuth2 + OIDC client flows (~authlib / oauthlib subset).

mod client;
mod discovery;
mod error;
mod id_token;
mod json_util;
mod parallel;
mod pkce;
mod random;
mod token;
mod url;

pub use client::{ClientAuthMethod, OAuthClient, OAuthClientBuilder};
pub use discovery::{discover, OidcConfig};
pub use error::{OAuthError, OAuthResult};
pub use id_token::{IdTokenValidation, VerifiedClaims};
pub use parallel::{parallel_client_credentials, parallel_refresh, ParallelOpts};
pub use pkce::{pkce_challenge, pkce_pair, PkceChallengeMethod, PkcePair};
pub use random::{random_nonce, random_state, random_verifier};
pub use token::{
    access_token, client_credentials, exchange_code, fetch_userinfo, introspect_token, is_bearer,
    parse_token_json, refresh_token, revoke_token, token_expired, token_expires_in, token_type,
    ClientCredentialsOptions, ExchangeOptions, RefreshOptions, TokenResponse,
};
pub use url::{
    auth_url, basic_auth_header, parse_authorization_response, parse_callback_url, validate_state,
    AuthUrlOptions, AuthorizationResponse,
};
