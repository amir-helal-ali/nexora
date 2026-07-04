//! SSO configuration — providers, endpoints, credentials.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The kind of SSO protocol a provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SsoProviderKind {
    /// OpenID Connect (OAuth 2.0 + ID tokens).
    Oidc,
    /// SAML 2.0 (XML assertions).
    Saml,
}

/// One SSO provider configuration (e.g. "google", "okta-prod").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoProviderConfig {
    /// Unique provider identifier (used in URLs).
    pub id: String,
    /// Display name shown on login page.
    pub display_name: String,
    /// Protocol kind.
    pub kind: SsoProviderKind,
    /// Client ID (OIDC) or entity ID (SAML).
    pub client_id: String,
    /// Client secret (OIDC) — stored encrypted at rest.
    pub client_secret: String,
    /// OIDC discovery URL (e.g. `https://accounts.google.com/.well-known/openid-configuration`).
    /// Ignored for SAML providers.
    #[serde(default)]
    pub oidc_discovery_url: Option<String>,
    /// OIDC scopes to request. Defaults to `["openid", "email", "profile"]`.
    #[serde(default = "default_scopes")]
    pub oidc_scopes: Vec<String>,
    /// SAML IdP metadata URL (XML). Ignored for OIDC providers.
    #[serde(default)]
    pub saml_metadata_url: Option<String>,
    /// SAML IdP SSO endpoint (where browser redirects to).
    #[serde(default)]
    pub saml_sso_url: Option<String>,
    /// SAML IdP X.509 certificate (PEM). Used to verify assertions.
    #[serde(default)]
    pub saml_idp_certificate: Option<String>,
    /// SAML SP entity ID (this platform's entity ID).
    #[serde(default)]
    pub saml_sp_entity_id: Option<String>,
    /// SAML SP ACS (Assertion Consumer Service) URL.
    #[serde(default)]
    pub saml_sp_acs_url: Option<String>,
    /// Where to send the user after a successful SSO login.
    pub redirect_after_login: String,
    /// Mapping of IdP claim → Nexora role.
    #[serde(default)]
    pub role_mapping: HashMap<String, String>,
    /// Default role if no mapping matches.
    #[serde(default = "default_role")]
    pub default_role: String,
}

fn default_scopes() -> Vec<String> {
    vec!["openid".into(), "email".into(), "profile".into()]
}

fn default_role() -> String {
    "viewer".into()
}

/// A configured SSO provider, identified by its ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoProvider {
    pub id: String,
    pub config: SsoProviderConfig,
}

/// The full SSO configuration for the platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoConfig {
    /// All registered SSO providers.
    pub providers: Vec<SsoProviderConfig>,
    /// How long SSO sessions are valid (seconds). Default: 8h.
    #[serde(default = "default_session_ttl")]
    pub session_ttl_seconds: u64,
}

impl Default for SsoConfig {
    fn default() -> Self {
        Self {
            providers: Vec::new(),
            session_ttl_seconds: default_session_ttl(),
        }
    }
}

fn default_session_ttl() -> u64 {
    8 * 3600
}

impl SsoConfig {
    /// Find a provider by ID.
    pub fn find(&self, id: &str) -> Option<&SsoProviderConfig> {
        self.providers.iter().find(|p| p.id == id)
    }

    /// Add or replace a provider.
    pub fn upsert(&mut self, provider: SsoProviderConfig) {
        if let Some(existing) = self.providers.iter_mut().find(|p| p.id == provider.id) {
            *existing = provider;
        } else {
            self.providers.push(provider);
        }
    }

    /// Remove a provider.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.providers.len();
        self.providers.retain(|p| p.id != id);
        self.providers.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_oidc() -> SsoProviderConfig {
        SsoProviderConfig {
            id: "google".into(),
            display_name: "Google Workspace".into(),
            kind: SsoProviderKind::Oidc,
            client_id: "xxx.apps.googleusercontent.com".into(),
            client_secret: "secret".into(),
            oidc_discovery_url: Some("https://accounts.google.com/.well-known/openid-configuration".into()),
            oidc_scopes: default_scopes(),
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
    fn find_provider_by_id() {
        let mut cfg = SsoConfig::default();
        cfg.upsert(sample_oidc());
        assert!(cfg.find("google").is_some());
        assert!(cfg.find("azure").is_none());
    }

    #[test]
    fn upsert_replaces_existing() {
        let mut cfg = SsoConfig::default();
        cfg.upsert(sample_oidc());
        let mut updated = sample_oidc();
        updated.display_name = "Google".into();
        cfg.upsert(updated);
        assert_eq!(cfg.providers.len(), 1);
        assert_eq!(cfg.find("google").unwrap().display_name, "Google");
    }

    #[test]
    fn remove_provider() {
        let mut cfg = SsoConfig::default();
        cfg.upsert(sample_oidc());
        assert!(cfg.remove("google"));
        assert!(!cfg.remove("google"));
        assert!(cfg.providers.is_empty());
    }

    #[test]
    fn default_session_ttl_is_8h() {
        let cfg = SsoConfig::default();
        assert_eq!(cfg.session_ttl_seconds, 8 * 3600);
    }

    #[test]
    fn serde_roundtrip() {
        let mut cfg = SsoConfig::default();
        cfg.upsert(sample_oidc());
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: SsoConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.providers.len(), cfg2.providers.len());
        assert_eq!(cfg2.providers[0].id, "google");
    }

    #[test]
    fn oidc_provider_kind_serializes_as_lowercase() {
        let p = sample_oidc();
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"kind\":\"oidc\""));
    }
}
