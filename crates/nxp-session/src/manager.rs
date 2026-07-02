//! Session manager — in-process registry of active NXP sessions.
//!
//! See RFC §2.2. Each session holds the AEAD contexts for both directions
//! plus metadata (expiry, peer identity, capabilities).

use crate::time::now_us;
use nxp_security::{FrameAead, SessionId};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Default session lifetime: 1 hour.
pub const DEFAULT_SESSION_TTL: Duration = Duration::from_secs(3600);

/// Heartbeat interval: 15 seconds.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

/// Number of missed heartbeats before a session is declared dead: 3.
pub const MAX_MISSED_HEARTBEATS: u32 = 3;

/// Live session state.
pub struct Session {
    /// Session ID (16 bytes).
    pub id: SessionId,
    /// Client → server AEAD.
    pub client_to_server: FrameAead,
    /// Server → client AEAD.
    pub server_to_client: FrameAead,
    /// Microsecond timestamp at which the session expires.
    pub expires_at_us: u64,
    /// Last time we observed traffic from the client (microseconds).
    pub last_seen_us: u64,
    /// Peer's long-term Ed25519 public key (32 bytes), if known.
    pub peer_identity: Option<[u8; 32]>,
    /// Negotiated capabilities bitmask.
    pub capabilities: u32,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("id", &hex::encode(self.id))
            .field("expires_at_us", &self.expires_at_us)
            .field("last_seen_us", &self.last_seen_us)
            .field("has_peer_identity", &self.peer_identity.is_some())
            .field("capabilities", &self.capabilities)
            .finish_non_exhaustive()
    }
}

/// External state of a session — returned by `SessionManager::info` for
/// observability and inspection. Does not expose keys.
#[derive(Debug, Clone)]
pub struct SessionState {
    /// Session ID (hex).
    pub id_hex: String,
    /// Microsecond timestamp at which the session expires.
    pub expires_at_us: u64,
    /// Last activity timestamp.
    pub last_seen_us: u64,
    /// Peer identity public key (hex), if known.
    pub peer_identity_hex: Option<String>,
    /// Negotiated capabilities.
    pub capabilities: u32,
    /// True if expired.
    pub is_expired: bool,
}

impl Session {
    /// Construct a new session from derived keys.
    pub fn new(keys: nxp_security::SessionKeys, peer_identity: Option<[u8; 32]>, capabilities: u32) -> Self {
        let now = now_us();
        Self {
            id: keys.session_id,
            client_to_server: FrameAead::new_receiver(&keys.client_to_server),
            server_to_client: FrameAead::new_sender(&keys.server_to_client),
            expires_at_us: now + DEFAULT_SESSION_TTL.as_micros() as u64,
            last_seen_us: now,
            peer_identity,
            capabilities,
        }
    }

    /// Returns `true` if the session has expired.
    pub fn is_expired(&self) -> bool {
        now_us() >= self.expires_at_us
    }

    /// Mark activity on this session.
    pub fn touch(&mut self) {
        self.last_seen_us = now_us();
    }

    /// Returns `true` if the session is considered dead due to inactivity.
    pub fn is_heartbeat_dead(&self) -> bool {
        let max_gap = HEARTBEAT_INTERVAL.as_micros() as u64 * MAX_MISSED_HEARTBEATS as u64;
        now_us().saturating_sub(self.last_seen_us) > max_gap
    }
}

/// Thread-safe in-process session registry.
#[derive(Default)]
pub struct SessionManager {
    sessions: RwLock<HashMap<SessionId, Arc<parking_lot::Mutex<Session>>>>,
}

impl std::fmt::Debug for SessionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.sessions.read().len();
        f.debug_struct("SessionManager")
            .field("active_sessions", &count)
            .finish()
    }
}

impl SessionManager {
    /// Construct an empty manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new session. Returns the session ID.
    pub fn insert(&self, session: Session) -> SessionId {
        let id = session.id;
        let arc = Arc::new(parking_lot::Mutex::new(session));
        self.sessions.write().insert(id, arc);
        id
    }

    /// Look up a session by ID. Returns `None` if not found or expired.
    pub fn get(&self, id: &SessionId) -> Option<Arc<parking_lot::Mutex<Session>>> {
        let sessions = self.sessions.read();
        let arc = sessions.get(id).cloned()?;
        // Check expiry under the lock.
        {
            let s = arc.lock();
            if s.is_expired() {
                return None;
            }
        }
        Some(arc)
    }

    /// Remove a session (e.g. on `BYE`).
    pub fn remove(&self, id: &SessionId) -> bool {
        self.sessions.write().remove(id).is_some()
    }

    /// Reap expired sessions. Returns the number removed.
    pub fn reap_expired(&self) -> usize {
        let mut sessions = self.sessions.write();
        let before = sessions.len();
        sessions.retain(|_, arc| {
            let s = arc.lock();
            !s.is_expired() && !s.is_heartbeat_dead()
        });
        before - sessions.len()
    }

    /// Snapshot all session states (for observability).
    pub fn snapshot(&self) -> Vec<SessionState> {
        let sessions = self.sessions.read();
        sessions
            .values()
            .map(|arc| {
                let s = arc.lock();
                SessionState {
                    id_hex: hex::encode(s.id),
                    expires_at_us: s.expires_at_us,
                    last_seen_us: s.last_seen_us,
                    peer_identity_hex: s.peer_identity.as_ref().map(hex::encode),
                    capabilities: s.capabilities,
                    is_expired: s.is_expired(),
                }
            })
            .collect()
    }

    /// Number of active sessions.
    pub fn len(&self) -> usize {
        self.sessions.read().len()
    }

    /// Returns `true` if there are no active sessions.
    pub fn is_empty(&self) -> bool {
        self.sessions.read().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nxp_security::{SessionKeys, SessionSecret};

    fn fake_keys() -> SessionKeys {
        let a = SessionSecret::generate();
        let b = SessionSecret::generate();
        a.derive(&b.public_key())
    }

    #[test]
    fn insert_get_remove() {
        let mgr = SessionManager::new();
        let keys = fake_keys();
        let id = mgr.insert(Session::new(keys, None, 0));
        assert_eq!(mgr.len(), 1);
        assert!(mgr.get(&id).is_some());
        assert!(mgr.remove(&id));
        assert_eq!(mgr.len(), 0);
        assert!(mgr.get(&id).is_none());
    }

    #[test]
    fn snapshot_works() {
        let mgr = SessionManager::new();
        let keys = fake_keys();
        let id = mgr.insert(Session::new(keys, None, 0xFF));
        let snap = mgr.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].id_hex, hex::encode(id));
        assert_eq!(snap[0].capabilities, 0xFF);
    }
}
