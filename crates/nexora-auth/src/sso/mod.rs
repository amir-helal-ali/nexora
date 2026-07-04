//! # Nexora SSO — SAML 2.0 + OIDC Enterprise Authentication
//!
//! This module provides enterprise-grade single sign-on (SSO) integration
//! for the Nexora platform. It supports two industry-standard protocols:
//!
//! - **OIDC (OpenID Connect)** — for cloud IdPs like Google, Microsoft Entra,
//!   Okta, Auth0.
//! - **SAML 2.0** — for legacy enterprise IdPs like ADFS, Shibboleth.
//!
//! # Architecture
//!
//! ```text
//! +---------+      +--------------+      +-------------+
//! |  User   |----->|  Nexora      |----->|  IdP        |
//! | Browser |      |  Gateway     |      |  (OIDC/SAML)|
//! |         |<-----|              |<-----|             |
//! |         |      |              |      |             |
//! |         |      |  nexora-auth |      |             |
//! |         |      |  SSO module  |      |             |
//! +---------+      +--------------+      +-------------+
//! ```
//!
//! # Flow (OIDC Authorization Code)
//!
//! 1. User visits `/auth/sso/oidc/{provider}/login`
//! 2. Gateway redirects to IdP's authorization endpoint
//! 3. User authenticates at IdP
//! 4. IdP redirects back to `/auth/sso/oidc/{provider}/callback?code=...`
//! 5. SSO module exchanges code for ID token + access token
//! 6. SSO module verifies ID token signature against IdP's JWKS
//! 7. SSO module extracts user identity (sub, email, name)
//! 8. SSO module mints a Nexora session token and redirects to dashboard
//!
//! # Feature Flag
//!
//! SSO is gated behind the `sso` feature flag because it pulls in
//! `reqwest` and `jsonwebtoken`. To enable:
//!
//! ```toml
//! [dependencies]
//! nexora-auth = { version = "...", features = ["sso"] }
//! ```

pub mod config;
pub mod error;
pub mod oidc;
pub mod saml;
pub mod session;

pub use config::{SsoConfig, SsoProvider, SsoProviderConfig, SsoProviderKind};
pub use error::{SsoError, SsoResult};
pub use oidc::OidcClient;
pub use saml::SamlClient;
pub use session::{SsoSession, SsoSessionManager};
