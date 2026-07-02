//! SSO session management — tracks in-flight SSO flows (state, nonce) and
//! completed SSO sessions (link to Nexora user).
//!
//! This is an in-memory store. Production deployments with multiple
//! gateway instances should use a shared cache (Redis or PostgreSQL).

use crate::sso::error::{SsoError, SsoResult};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use time::OffsetDateTime;
use uuid::Uuid;

/// An in-flight SSO flow (between auth request and callback).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingFlow {
    /// Random state string sent to the IdP.
    pub state: String,
    /// Provider ID (e.g. "google").
    pub provider_id: String,
    /// Redirect URI registered with the IdP.
    pub redirect_uri: String,
    /// Nonce (OIDC only) — for replay protection.
    pub nonce: Option<String>,
    /// PKCE verifier (OIDC only).
    pub pkce_verifier: Option<String>,
    /// When this flow was started (unix nanos).
    pub started_at: i64,
    /// When this flow expires (unix nanos). Typically 5 minutes.
    pub expires_at: i64,
}

/// A completed SSO session — the user has authenticated via SSO and we've
/// linked them to a Nexora user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoSession {
    /// Session ID (used as a cookie value).
    pub id: String,
    /// Nexora user ID this session is bound to.
    pub user_id: String,
    /// SSO provider that authenticated the user.
    pub provider_id: String,
    /// IdP subject (e.g. OIDC `sub` or SAML NameID).
    pub idp_subject: String,
    /// When the session was created (unix nanos).
    pub created_at: i64,
    /// When the session expires (unix nanos).
    pub expires_at: i64,
}

/// In-memory SSO session manager.
pub struct SsoSessionManager {
    flows: RwLock<HashMap<String, PendingFlow>>,
    sessions: RwLock<HashMap<String, SsoSession>>,
    /// Default flow TTL (5 minutes).
    flow_ttl_seconds: i64,
    /// Default session TTL (8 hours).
    session_ttl_seconds: i64,
}

impl Default for SsoSessionManager {
    fn default() -> Self {
        Self::new(5 * 60, 8 * 3600)
    }
}

impl SsoSessionManager {
    pub fn new(flow_ttl_seconds: i64, session_ttl_seconds: i64) -> Self {
        Self {
            flows: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            flow_ttl_seconds,
            session_ttl_seconds,
        }
    }

    /// Begin a new SSO flow. Returns the state to send to the IdP.
    pub fn start_flow(
        &self,
        provider_id: &str,
        redirect_uri: &str,
        nonce: Option<String>,
        pkce_verifier: Option<String>,
    ) -> PendingFlow {
        let state = random_state();
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let flow = PendingFlow {
            state: state.clone(),
            provider_id: provider_id.to_string(),
            redirect_uri: redirect_uri.to_string(),
            nonce,
            pkce_verifier,
            started_at: now,
            expires_at: now + flow_ttl_seconds_to_nanos(self.flow_ttl_seconds),
        };
        self.flows.write().insert(state, flow.clone());
        flow
    }

    /// Consume a pending flow (verifying the state matches). Returns the
    /// flow on success. The flow is removed after consumption (one-shot).
    pub fn consume_flow(&self, state: &str) -> SsoResult<PendingFlow> {
        let mut flows = self.flows.write();
        let flow = flows
            .remove(state)
            .ok_or_else(|| SsoError::StateMismatch)?;
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        if now > flow.expires_at {
            return Err(SsoError::SessionExpired);
        }
        Ok(flow)
    }

    /// Create a completed SSO session.
    pub fn create_session(
        &self,
        user_id: &str,
        provider_id: &str,
        idp_subject: &str,
    ) -> SsoSession {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let session = SsoSession {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            provider_id: provider_id.to_string(),
            idp_subject: idp_subject.to_string(),
            created_at: now,
            expires_at: now + session_ttl_seconds_to_nanos(self.session_ttl_seconds),
        };
        self.sessions
            .write()
            .insert(session.id.clone(), session.clone());
        session
    }

    /// Look up a session by ID. Returns None if not found or expired.
    pub fn get_session(&self, id: &str) -> Option<SsoSession> {
        let sessions = self.sessions.read();
        let session = sessions.get(id)?;
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        if now > session.expires_at {
            return None;
        }
        Some(session.clone())
    }

    /// Revoke a session.
    pub fn revoke_session(&self, id: &str) -> bool {
        self.sessions.write().remove(id).is_some()
    }

    /// Purge expired flows and sessions. Returns (flows_purged, sessions_purged).
    pub fn purge_expired(&self) -> (usize, usize) {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let mut flows = self.flows.write();
        let before_flows = flows.len();
        flows.retain(|_, f| f.expires_at > now);
        let flows_purged = before_flows - flows.len();

        let mut sessions = self.sessions.write();
        let before_sessions = sessions.len();
        sessions.retain(|_, s| s.expires_at > now);
        let sessions_purged = before_sessions - sessions.len();

        (flows_purged, sessions_purged)
    }

    /// Count of pending flows (for diagnostics).
    pub fn flow_count(&self) -> usize {
        self.flows.read().len()
    }

    /// Count of active sessions (for diagnostics).
    pub fn session_count(&self) -> usize {
        self.sessions.read().len()
    }
}

fn flow_ttl_seconds_to_nanos(s: i64) -> i64 {
    s * 1_000_000_000
}

fn session_ttl_seconds_to_nanos(s: i64) -> i64 {
    s * 1_000_000_000
}

fn random_state() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_and_consume_flow() {
        let mgr = SsoSessionManager::default();
        let flow = mgr.start_flow("google", "https://nexora.dev/cb", Some("nonce-123".into()), None);
        assert_eq!(mgr.flow_count(), 1);
        let consumed = mgr.consume_flow(&flow.state).unwrap();
        assert_eq!(consumed.provider_id, "google");
        assert_eq!(consumed.nonce, Some("nonce-123".into()));
        // Flow is one-shot.
        assert!(mgr.consume_flow(&flow.state).is_err());
        assert_eq!(mgr.flow_count(), 0);
    }

    #[test]
    fn consume_flow_rejects_unknown_state() {
        let mgr = SsoSessionManager::default();
        assert!(matches!(
            mgr.consume_flow("nonexistent"),
            Err(SsoError::StateMismatch)
        ));
    }

    #[test]
    fn create_and_get_session() {
        let mgr = SsoSessionManager::default();
        let session = mgr.create_session("user-1", "google", "sub-abc");
        assert_eq!(mgr.session_count(), 1);
        let fetched = mgr.get_session(&session.id).unwrap();
        assert_eq!(fetched.user_id, "user-1");
        assert_eq!(fetched.provider_id, "google");
        assert_eq!(fetched.idp_subject, "sub-abc");
    }

    #[test]
    fn revoke_session() {
        let mgr = SsoSessionManager::default();
        let session = mgr.create_session("user-1", "google", "sub");
        assert!(mgr.revoke_session(&session.id));
        assert!(!mgr.revoke_session(&session.id));
        assert!(mgr.get_session(&session.id).is_none());
    }

    #[test]
    fn purge_expired_removes_old_flows_and_sessions() {
        let mgr = SsoSessionManager::new(0, 0); // immediate expiry
        std::thread::sleep(std::time::Duration::from_millis(10));
        let _flow = mgr.start_flow("google", "https://x", None, None);
        let _session = mgr.create_session("u", "google", "s");
        std::thread::sleep(std::time::Duration::from_millis(10));
        let (flows, sessions) = mgr.purge_expired();
        assert!(flows >= 1);
        assert!(sessions >= 1);
    }

    #[test]
    fn expired_flow_rejected_on_consume() {
        let mgr = SsoSessionManager::new(0, 0);
        let flow = mgr.start_flow("google", "https://x", None, None);
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(matches!(
            mgr.consume_flow(&flow.state),
            Err(SsoError::SessionExpired)
        ));
    }

    #[test]
    fn get_session_returns_none_for_expired() {
        let mgr = SsoSessionManager::new(60, 0); // session TTL = 0
        let session = mgr.create_session("u", "google", "s");
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(mgr.get_session(&session.id).is_none());
    }
}
