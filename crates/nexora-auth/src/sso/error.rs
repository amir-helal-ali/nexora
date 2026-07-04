//! SSO error types.

use thiserror::Error;

pub type SsoResult<T> = Result<T, SsoError>;

#[derive(Debug, Error)]
pub enum SsoError {
    #[error("SSO provider not configured: {0}")]
    ProviderNotConfigured(String),

    #[error("SSO provider not found: {0}")]
    ProviderNotFound(String),

    #[error("invalid redirect URL: {0}")]
    InvalidRedirectUrl(String),

    #[error("OIDC discovery failed: {0}")]
    OidcDiscoveryFailed(String),

    #[error("OIDC token exchange failed: {0}")]
    OidcTokenExchangeFailed(String),

    #[error("OIDC ID token verification failed: {0}")]
    OidcTokenVerificationFailed(String),

    #[error("OIDC JWKS fetch failed: {0}")]
    OidcJwksFetchFailed(String),

    #[error("SAML response invalid: {0}")]
    SamlResponseInvalid(String),

    #[error("SAML signature verification failed: {0}")]
    SamlSignatureInvalid(String),

    #[error("SAML response expired")]
    SamlExpired,

    #[error("SAML response replayed")]
    SamlReplayed,

    #[error("SSO state mismatch (CSRF protection)")]
    StateMismatch,

    #[error("SSO session expired")]
    SessionExpired,

    #[error("SSO session not found: {0}")]
    SessionNotFound(String),

    #[error("IdP did not return required claim: {0}")]
    MissingClaim(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("JSON error: {0}")]
    Json(String),

    #[error("base64 decode error: {0}")]
    Base64(String),

    #[error("URL parse error: {0}")]
    Url(String),

    #[error("SSO feature not enabled (rebuild with --features sso)")]
    FeatureNotEnabled,
}

#[cfg(feature = "reqwest")]
impl From<reqwest::Error> for SsoError {
    fn from(e: reqwest::Error) -> Self {
        SsoError::Http(e.to_string())
    }
}

impl From<serde_json::Error> for SsoError {
    fn from(e: serde_json::Error) -> Self {
        SsoError::Json(e.to_string())
    }
}

impl From<url::ParseError> for SsoError {
    fn from(e: url::ParseError) -> Self {
        SsoError::Url(e.to_string())
    }
}

impl From<base64::DecodeError> for SsoError {
    fn from(e: base64::DecodeError) -> Self {
        SsoError::Base64(e.to_string())
    }
}
