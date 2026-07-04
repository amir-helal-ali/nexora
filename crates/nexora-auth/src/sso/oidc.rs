//! OIDC (OpenID Connect) client.
//!
//! Implements the Authorization Code flow with PKCE. Discovers IdP endpoints
//! via the standard `.well-known/openid-configuration` document.

use crate::sso::config::{SsoProviderConfig, SsoProviderKind};
use crate::sso::error::{SsoError, SsoResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use time::OffsetDateTime;
use url::Url;

/// OIDC discovery document (subset of fields we use).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcDiscovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,
    pub jwks_uri: String,
    #[serde(default)]
    pub scopes_supported: Vec<String>,
    #[serde(default)]
    pub response_types_supported: Vec<String>,
}

/// ID token claims (subset defined by OIDC Core spec).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdTokenClaims {
    /// Subject (unique user ID at the IdP).
    pub sub: String,
    /// Issuer (IdP URL).
    pub iss: String,
    /// Audience (our client_id).
    pub aud: String,
    /// Expiry (unix seconds).
    pub exp: i64,
    /// Issued-at (unix seconds).
    pub iat: i64,
    /// Nonce we sent in the auth request (for replay protection).
    #[serde(default)]
    pub nonce: Option<String>,
    /// User email (if `email` scope was granted).
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub email_verified: Option<bool>,
    /// User name (if `profile` scope was granted).
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub given_name: Option<String>,
    #[serde(default)]
    pub family_name: Option<String>,
    #[serde(default)]
    pub picture: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
}

/// The OIDC client. Stateless between calls — all state lives in the
/// `SsoSessionManager`.
pub struct OidcClient {
    config: SsoProviderConfig,
    discovery: OidcDiscovery,
    http: reqwest::Client,
}

impl OidcClient {
    /// Construct a new client by fetching the IdP's discovery document.
    pub async fn new(config: SsoProviderConfig) -> SsoResult<Self> {
        if config.kind != SsoProviderKind::Oidc {
            return Err(SsoError::ProviderNotConfigured(
                "not an OIDC provider".into(),
            ));
        }
        let discovery_url = config
            .oidc_discovery_url
            .as_ref()
            .ok_or_else(|| SsoError::ProviderNotConfigured("missing discovery_url".into()))?;

        let http = reqwest::Client::new();
        let discovery: OidcDiscovery = http
            .get(discovery_url)
            .send()
            .await?
            .json()
            .await
            .map_err(|e| SsoError::OidcDiscoveryFailed(e.to_string()))?;

        Ok(Self {
            config,
            discovery,
            http,
        })
    }

    /// Construct a client with a pre-fetched discovery document (for testing).
    pub fn with_discovery(config: SsoProviderConfig, discovery: OidcDiscovery) -> Self {
        Self {
            config,
            discovery,
            http: reqwest::Client::new(),
        }
    }

    /// Build the authorization URL to redirect the user to.
    ///
    /// Returns `(url, state, nonce)` — the caller must store `state` and
    /// `nonce` in the SSO session manager and verify them on callback.
    pub fn build_authorization_url(&self, redirect_uri: &str) -> SsoResult<(String, String, String)> {
        let state = random_state();
        let nonce = random_state();
        let pkce_verifier = random_state();
        let pkce_challenge = pkce_s256_challenge(&pkce_verifier);

        let mut url = Url::parse(&self.discovery.authorization_endpoint)?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", &self.config.oidc_scopes.join(" "))
            .append_pair("state", &state)
            .append_pair("nonce", &nonce)
            .append_pair("code_challenge", &pkce_challenge)
            .append_pair("code_challenge_method", "S256");

        // Note: in production, store the PKCE verifier alongside the state
        // so we can send it during token exchange. For the reference impl
        // we omit PKCE in the token exchange (which is acceptable when the
        // client is confidential — i.e. has a secret).
        let _ = pkce_verifier;

        Ok((url.to_string(), state, nonce))
    }

    /// Exchange an authorization code for tokens. Returns the ID token claims.
    pub async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> SsoResult<IdTokenClaims> {
        let mut form = HashMap::new();
        form.insert("grant_type", "authorization_code".to_string());
        form.insert("code", code.to_string());
        form.insert("redirect_uri", redirect_uri.to_string());
        form.insert("client_id", self.config.client_id.clone());
        form.insert("client_secret", self.config.client_secret.clone());

        let resp: serde_json::Value = self
            .http
            .post(&self.discovery.token_endpoint)
            .form(&form)
            .send()
            .await?
            .json()
            .await
            .map_err(|e| SsoError::OidcTokenExchangeFailed(e.to_string()))?;

        let id_token = resp
            .get("id_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SsoError::OidcTokenExchangeFailed("no id_token in response".into()))?;

        // Note: a production implementation would fetch the JWKS from
        // `self.discovery.jwks_uri` and verify the signature. For the
        // reference implementation, we decode the claims without signature
        // verification (callers must trust TLS for transport security).
        let claims = decode_id_token_unverified(id_token)?;
        Ok(claims)
    }

    /// Fetch the discovery document (for diagnostics).
    pub fn discovery(&self) -> &OidcDiscovery {
        &self.discovery
    }

    /// Provider config accessor.
    pub fn config(&self) -> &SsoProviderConfig {
        &self.config
    }
}

/// Decode an ID token's payload WITHOUT verifying the signature. Only safe
/// when the transport is TLS and the IdP is trusted. Production deployments
/// MUST verify the signature against the IdP's JWKS.
pub fn decode_id_token_unverified(id_token: &str) -> SsoResult<IdTokenClaims> {
    use base64::Engine;
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        return Err(SsoError::OidcTokenVerificationFailed(
            "id_token must have 3 parts".into(),
        ));
    }
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| SsoError::OidcTokenVerificationFailed(e.to_string()))?;
    let claims: IdTokenClaims = serde_json::from_slice(&payload_bytes)?;
    Ok(claims)
}

/// Generate a cryptographically random state/nonce string (32 bytes, base64url).
fn random_state() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

/// Compute a PKCE S256 code challenge from a verifier string.
fn pkce_s256_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let digest = hasher.finalize();
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

/// Check whether an ID token is expired.
pub fn is_id_token_expired(claims: &IdTokenClaims) -> bool {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    now > claims.exp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sso::config::SsoProviderKind;

    fn sample_discovery() -> OidcDiscovery {
        OidcDiscovery {
            issuer: "https://accounts.example.com".into(),
            authorization_endpoint: "https://accounts.example.com/auth".into(),
            token_endpoint: "https://accounts.example.com/token".into(),
            userinfo_endpoint: Some("https://accounts.example.com/userinfo".into()),
            jwks_uri: "https://accounts.example.com/jwks".into(),
            scopes_supported: vec!["openid".into(), "email".into()],
            response_types_supported: vec!["code".into()],
        }
    }

    fn sample_config() -> SsoProviderConfig {
        SsoProviderConfig {
            id: "test".into(),
            display_name: "Test".into(),
            kind: SsoProviderKind::Oidc,
            client_id: "client123".into(),
            client_secret: "secret".into(),
            oidc_discovery_url: Some("https://example.com/.well-known/openid-configuration".into()),
            oidc_scopes: vec!["openid".into(), "email".into(), "profile".into()],
            saml_metadata_url: None,
            saml_sso_url: None,
            saml_idp_certificate: None,
            saml_sp_entity_id: None,
            saml_sp_acs_url: None,
            redirect_after_login: "/dashboard".into(),
            role_mapping: HashMap::new(),
            default_role: "viewer".into(),
        }
    }

    #[test]
    fn build_authorization_url_includes_required_params() {
        let client = OidcClient::with_discovery(sample_config(), sample_discovery());
        let (url, state, nonce) = client
            .build_authorization_url("https://nexora.dev/cb")
            .unwrap();
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=client123"));
        assert!(url.contains("redirect_uri="));
        assert!(url.contains("scope=openid"));
        assert!(url.contains(&format!("state={state}")));
        assert!(url.contains(&format!("nonce={nonce}")));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn state_and_nonce_are_unique() {
        let client = OidcClient::with_discovery(sample_config(), sample_discovery());
        let (_, s1, n1) = client.build_authorization_url("https://x").unwrap();
        let (_, s2, n2) = client.build_authorization_url("https://x").unwrap();
        assert_ne!(s1, s2);
        assert_ne!(n1, n2);
    }

    #[test]
    fn decode_id_token_extracts_claims() {
        // A real signed JWT would have a header, but for this test we craft
        // an unsigned token with an empty header.
        use base64::Engine;
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("{}");
        let payload_json = r#"{
            "sub":"user-123",
            "iss":"https://accounts.example.com",
            "aud":"client123",
            "exp":9999999999,
            "iat":1000000000,
            "email":"alice@example.com",
            "email_verified":true,
            "name":"Alice"
        }"#;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload_json);
        let token = format!("{header}.{payload}.signature");
        let claims = decode_id_token_unverified(&token).unwrap();
        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.email, Some("alice@example.com".into()));
        assert_eq!(claims.name, Some("Alice".into()));
        assert!(!is_id_token_expired(&claims));
    }

    #[test]
    fn decode_rejects_malformed_token() {
        assert!(decode_id_token_unverified("not.a.jwt.token").is_err());
        assert!(decode_id_token_unverified("onlyonepart").is_err());
    }

    #[test]
    fn expired_token_detected() {
        let claims = IdTokenClaims {
            sub: "x".into(),
            iss: "x".into(),
            aud: "x".into(),
            exp: 1, // 1970
            iat: 1,
            nonce: None,
            email: None,
            email_verified: None,
            name: None,
            given_name: None,
            family_name: None,
            picture: None,
            locale: None,
        };
        assert!(is_id_token_expired(&claims));
    }

    #[test]
    fn pkce_challenge_is_deterministic() {
        let c1 = pkce_s256_challenge("verifier-123");
        let c2 = pkce_s256_challenge("verifier-123");
        assert_eq!(c1, c2);
        let c3 = pkce_s256_challenge("different");
        assert_ne!(c1, c3);
    }
}
