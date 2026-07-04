//! Session store — tracks active sessions per user.
//!
//! See Nexora Engineering Specification, Part 9 (AUTHENTICATION SYSTEM).
//! Sessions are short-lived (default 1h) and rotated. Each session holds
//! a reference to the user ID and metadata (issued-at, last-active).

use crate::users::UserId;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;
use time::OffsetDateTime;

/// Unique session ID (UUID v4).
pub type SessionId = String;

/// A live session.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID.
    pub id: SessionId,
    /// User ID this session belongs to.
    pub user_id: UserId,
    /// When the session was created (unix nanos).
    pub created_at: i64,
    /// Last activity timestamp (unix nanos).
    pub last_active: i64,
    /// Whether the session is currently active.
    pub active: bool,
    /// Optional client IP / device fingerprint.
    pub client: Option<String>,
}

/// Session store. Thread-safe.
pub struct SessionStore {
    sessions: RwLock<HashMap<SessionId, Session>>,
    by_user: RwLock<HashMap<UserId, Vec<SessionId>>>,
    /// Default session TTL.
    pub default_ttl: Duration,
}

impl fmt::Debug for SessionStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.sessions.read().len();
        f.debug_struct("SessionStore")
            .field("session_count", &count)
            .field("default_ttl_secs", &self.default_ttl.as_secs())
            .finish()
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore {
    /// Construct an empty session store with 1h default TTL.
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            by_user: RwLock::new(HashMap::new()),
            default_ttl: Duration::from_secs(3600),
        }
    }

    /// Number of active sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.read().len()
    }

    /// Create a new session for a user. Returns the session.
    pub fn create(&self, user_id: &str, client: Option<String>) -> Session {
        let id = uuid::Uuid::new_v4().to_string();
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let session = Session {
            id: id.clone(),
            user_id: user_id.to_string(),
            created_at: now,
            last_active: now,
            active: true,
            client,
        };
        let mut sessions = self.sessions.write();
        let mut by_user = self.by_user.write();
        sessions.insert(id.clone(), session.clone());
        by_user.entry(user_id.to_string()).or_default().push(id);
        session
    }

    /// Mark a session as inactive (logout).
    pub fn revoke(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write();
        if let Some(session) = sessions.get_mut(session_id) {
            session.active = false;
            return true;
        }
        false
    }

    /// Revoke all sessions for a user (e.g. on password change).
    pub fn revoke_all_for_user(&self, user_id: &str) -> usize {
        let by_user = self.by_user.read();
        let ids = by_user.get(user_id).cloned().unwrap_or_default();
        drop(by_user);
        let mut sessions = self.sessions.write();
        let mut count = 0;
        for id in &ids {
            if let Some(session) = sessions.get_mut(id) {
                session.active = false;
                count += 1;
            }
        }
        count
    }

    /// Update the last-active timestamp for a session.
    pub fn touch(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write();
        if let Some(session) = sessions.get_mut(session_id) {
            session.last_active = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
            return true;
        }
        false
    }

    /// Get a session by ID.
    pub fn get(&self, session_id: &str) -> Option<Session> {
        self.sessions.read().get(session_id).cloned()
    }

    /// List all sessions for a user.
    pub fn list_for_user(&self, user_id: &str) -> Vec<Session> {
        let by_user = self.by_user.read();
        let sessions = self.sessions.read();
        by_user
            .get(user_id)
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|id| sessions.get(id).cloned())
            .collect()
    }

    /// List all active sessions.
    pub fn list_active(&self) -> Vec<Session> {
        self.sessions
            .read()
            .values()
            .filter(|s| s.active)
            .cloned()
            .collect()
    }

    /// Clean up sessions older than the given TTL. Returns the number removed.
    pub fn reap_expired(&self, max_age: Duration) -> usize {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let cutoff = now - max_age.as_nanos() as i64;
        let mut sessions = self.sessions.write();
        let mut by_user = self.by_user.write();
        let before = sessions.len();
        let to_remove: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.last_active < cutoff)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &to_remove {
            if let Some(session) = sessions.remove(id) {
                if let Some(ids) = by_user.get_mut(&session.user_id) {
                    ids.retain(|x| x != id);
                }
            }
        }
        before - sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_get() {
        let store = SessionStore::new();
        let s = store.create("user-1", Some("client-A".into()));
        assert_eq!(s.user_id, "user-1");
        assert!(s.active);
        assert_eq!(store.session_count(), 1);
        let fetched = store.get(&s.id).unwrap();
        assert_eq!(fetched.user_id, "user-1");
    }

    #[test]
    fn revoke_marks_inactive() {
        let store = SessionStore::new();
        let s = store.create("user-1", None);
        assert!(store.revoke(&s.id));
        let fetched = store.get(&s.id).unwrap();
        assert!(!fetched.active);
    }

    #[test]
    fn revoke_all_for_user() {
        let store = SessionStore::new();
        store.create("user-1", None);
        store.create("user-1", None);
        store.create("user-2", None);
        let revoked = store.revoke_all_for_user("user-1");
        assert_eq!(revoked, 2);
        let active = store.list_active();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].user_id, "user-2");
    }

    #[test]
    fn list_for_user() {
        let store = SessionStore::new();
        store.create("user-1", None);
        store.create("user-1", None);
        store.create("user-2", None);
        assert_eq!(store.list_for_user("user-1").len(), 2);
        assert_eq!(store.list_for_user("user-2").len(), 1);
        assert_eq!(store.list_for_user("nobody").len(), 0);
    }

    #[test]
    fn touch_updates_timestamp() {
        let store = SessionStore::new();
        let s = store.create("user-1", None);
        let original = s.last_active;
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(store.touch(&s.id));
        let fetched = store.get(&s.id).unwrap();
        assert!(fetched.last_active > original);
    }

    #[test]
    fn reap_removes_old_sessions() {
        let store = SessionStore::new();
        let s = store.create("user-1", None);
        // Manually backdate the last_active.
        store.sessions.write().get_mut(&s.id).unwrap().last_active = 0;
        let reaped = store.reap_expired(Duration::from_secs(60));
        assert_eq!(reaped, 1);
        assert_eq!(store.session_count(), 0);
    }
}
