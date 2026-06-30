//! User store — in-memory user database with Argon2 password hashing.
//!
//! See Nexora Engineering Specification, Part 9 (IDENTITY SYSTEM).
//! Each user has:
//! - A unique user ID (UUID v4)
//! - A unique username (case-insensitive)
//! - An Argon2-hashed password
//! - An optional email
//! - A list of roles assigned to them (synced with the Permission Engine)
//! - Creation / last-login timestamps

use crate::password::{hash_password, verify_password, HashedPassword, PasswordError};
use nexora_core::events::EventPayload;
use nexora_core::permissions::{PermissionEngine, Principal, PrincipalKind};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

/// Unique user identifier (UUID v4).
pub type UserId = String;

/// A registered user.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    /// Unique user ID.
    pub id: UserId,
    /// Unique username (case-insensitive, stored lowercase).
    pub username: String,
    /// Argon2-hashed password (PHC string).
    #[serde(skip)]
    pub password_hash: Option<HashedPassword>,
    /// Optional email address.
    pub email: Option<String>,
    /// Roles assigned to this user.
    pub roles: Vec<String>,
    /// When the user was created (unix nanos).
    pub created_at: i64,
    /// Last successful login (unix nanos), or `None`.
    pub last_login: Option<i64>,
    /// Whether the user is currently active.
    pub active: bool,
}

/// Error from user operations.
#[derive(Debug, thiserror::Error)]
pub enum UserError {
    /// Username already taken.
    #[error("username already exists: {0}")]
    UsernameTaken(String),
    /// User not found.
    #[error("user not found: {0}")]
    NotFound(UserId),
    /// Invalid credentials (wrong password).
    #[error("invalid credentials")]
    InvalidCredentials,
    /// Password hashing failed.
    #[error("password error: {0}")]
    Password(#[from] PasswordError),
    /// User is inactive / disabled.
    #[error("user is inactive: {0}")]
    Inactive(UserId),
}

/// User store. Thread-safe.
pub struct UserStore {
    users: RwLock<HashMap<UserId, User>>,
    by_username: RwLock<HashMap<String, UserId>>,
    permission_engine: Option<Arc<PermissionEngine>>,
    event_bus: Option<Arc<nexora_core::EventBus>>,
}

impl fmt::Debug for UserStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.users.read().len();
        f.debug_struct("UserStore")
            .field("user_count", &count)
            .finish()
    }
}

impl Default for UserStore {
    fn default() -> Self {
        Self::new()
    }
}

impl UserStore {
    /// Construct an empty user store.
    pub fn new() -> Self {
        Self {
            users: RwLock::new(HashMap::new()),
            by_username: RwLock::new(HashMap::new()),
            permission_engine: None,
            event_bus: None,
        }
    }

    /// Attach a Permission Engine so user creation auto-registers a Principal.
    pub fn with_permission_engine(mut self, engine: Arc<PermissionEngine>) -> Self {
        self.permission_engine = Some(engine);
        self
    }

    /// Attach an Event Bus so lifecycle changes publish events.
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Number of registered users.
    pub fn user_count(&self) -> usize {
        self.users.read().len()
    }

    /// Create a new user. Hashes the password with Argon2 and auto-registers
    /// a Principal in the Permission Engine.
    pub fn create(
        &self,
        username: impl Into<String>,
        password: &str,
        email: Option<String>,
        roles: Vec<String>,
    ) -> Result<User, UserError> {
        let username = username.into().to_lowercase();
        let password_hash = hash_password(password)?;

        let id = Uuid::new_v4().to_string();
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;

        let user = User {
            id: id.clone(),
            username: username.clone(),
            password_hash: Some(password_hash),
            email,
            roles: roles.clone(),
            created_at: now,
            last_login: None,
            active: true,
        };

        // Insert into stores.
        {
            let mut users = self.users.write();
            let mut by_username = self.by_username.write();
            if by_username.contains_key(&username) {
                return Err(UserError::UsernameTaken(username));
            }
            users.insert(id.clone(), user.clone());
            by_username.insert(username, id.clone());
        }

        // Register a Principal in the Permission Engine.
        if let Some(engine) = &self.permission_engine {
            engine.register_principal(Principal {
                id: id.clone(),
                name: user.username.clone(),
                kind: PrincipalKind::User,
                roles: roles.into_iter().collect(),
            });
        }

        // Emit user.created event.
        if let Some(bus) = &self.event_bus {
            bus.publish_event("user.created".to_string(), EventPayload::Text(id.clone()));
        }

        Ok(user)
    }

    /// Get a user by ID.
    pub fn get(&self, id: &str) -> Option<User> {
        self.users.read().get(id).cloned()
    }

    /// Get a user by username (case-insensitive).
    pub fn get_by_username(&self, username: &str) -> Option<User> {
        let username = username.to_lowercase();
        let by_username = self.by_username.read();
        let id = by_username.get(&username)?;
        self.users.read().get(id).cloned()
    }

    /// Verify credentials. Returns the user if password matches.
    pub fn verify(&self, username: &str, password: &str) -> Result<User, UserError> {
        let user = self
            .get_by_username(username)
            .ok_or(UserError::NotFound(String::new()))?;
        if !user.active {
            return Err(UserError::Inactive(user.id));
        }
        let stored = user
            .password_hash
            .as_ref()
            .ok_or(UserError::InvalidCredentials)?;
        verify_password(password, stored).map_err(|_| UserError::InvalidCredentials)?;
        Ok(user)
    }

    /// Record a successful login (updates `last_login`).
    pub fn record_login(&self, id: &str) -> Option<User> {
        let mut users = self.users.write();
        let user = users.get_mut(id)?;
        user.last_login = Some(OffsetDateTime::now_utc().unix_timestamp_nanos() as i64);
        let user = user.clone();
        drop(users);
        if let Some(bus) = &self.event_bus {
            bus.publish_event("user.logged_in".to_string(), EventPayload::Text(id.to_string()));
        }
        Some(user)
    }

    /// Delete a user.
    pub fn delete(&self, id: &str) -> Result<(), UserError> {
        let mut users = self.users.write();
        let mut by_username = self.by_username.write();
        let user = users
            .remove(id)
            .ok_or_else(|| UserError::NotFound(id.to_string()))?;
        by_username.remove(&user.username);
        drop(users);
        drop(by_username);
        if let Some(bus) = &self.event_bus {
            bus.publish_event("user.deleted".to_string(), EventPayload::Text(id.to_string()));
        }
        Ok(())
    }

    /// List all users (snapshot).
    pub fn list(&self) -> Vec<User> {
        self.users.read().values().cloned().collect()
    }

    /// Insert a pre-built user directly (bypasses password hashing).
    /// Used by the persistence layer to restore state from SQLite on startup.
    /// Does NOT emit events or register principals (callers must do that
    /// separately if needed).
    pub fn insert_raw(&self, user: User) {
        let id = user.id.clone();
        let username = user.username.clone();
        self.users.write().insert(id.clone(), user);
        self.by_username.write().insert(username, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with_core() -> (UserStore, Arc<PermissionEngine>, Arc<nexora_core::EventBus>) {
        let engine = Arc::new(PermissionEngine::new());
        let bus = Arc::new(nexora_core::EventBus::new());
        let store = UserStore::new()
            .with_permission_engine(engine.clone())
            .with_event_bus(bus.clone());
        (store, engine, bus)
    }

    #[test]
    fn create_and_get_user() {
        let (store, _, _) = store_with_core();
        let user = store.create("Alice", "hunter2", Some("alice@nexora.io".into()), vec!["viewer".into()]).unwrap();
        assert_eq!(user.username, "alice"); // lowercase
        let by_id = store.get(&user.id).unwrap();
        assert_eq!(by_id.id, user.id);
        let by_name = store.get_by_username("ALICE").unwrap(); // case-insensitive
        assert_eq!(by_name.id, user.id);
    }

    #[test]
    fn duplicate_username_rejected() {
        let (store, _, _) = store_with_core();
        store.create("alice", "pw1", None, vec![]).unwrap();
        assert!(matches!(
            store.create("ALICE", "pw2", None, vec![]),
            Err(UserError::UsernameTaken(_))
        ));
    }

    #[test]
    fn verify_password_works() {
        let (store, _, _) = store_with_core();
        store.create("bob", "correct pw", None, vec![]).unwrap();
        assert!(store.verify("bob", "correct pw").is_ok());
        assert!(matches!(
            store.verify("bob", "wrong"),
            Err(UserError::InvalidCredentials)
        ));
    }

    #[test]
    fn inactive_user_cannot_login() {
        let (store, _, _) = store_with_core();
        let user = store.create("carol", "pw", None, vec![]).unwrap();
        // Manually mark as inactive.
        store.users.write().get_mut(&user.id).unwrap().active = false;
        assert!(matches!(
            store.verify("carol", "pw"),
            Err(UserError::Inactive(_))
        ));
    }

    #[test]
    fn create_emits_event() {
        let (store, _, bus) = store_with_core();
        store.create("dave", "pw", None, vec![]).unwrap();
        let events = bus.replay_filtered(0, "user.");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "user.created");
    }

    #[test]
    fn record_login_emits_event() {
        let (store, _, bus) = store_with_core();
        let user = store.create("eve", "pw", None, vec![]).unwrap();
        store.record_login(&user.id);
        let events = bus.replay_filtered(0, "user.");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].name, "user.created");
        assert_eq!(events[1].name, "user.logged_in");
    }

    #[test]
    fn delete_emits_event_and_removes_from_indices() {
        let (store, _, bus) = store_with_core();
        let user = store.create("frank", "pw", None, vec![]).unwrap();
        store.delete(&user.id).unwrap();
        assert!(store.get(&user.id).is_none());
        assert!(store.get_by_username("frank").is_none());
        let events = bus.replay_filtered(0, "user.");
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].name, "user.deleted");
    }

    #[test]
    fn create_registers_principal() {
        let (store, engine, _) = store_with_core();
        let user = store.create("grace", "pw", None, vec!["admin".into()]).unwrap();
        let principals = engine.list_principals();
        assert_eq!(principals.len(), 1);
        assert_eq!(principals[0].id, user.id);
        assert_eq!(principals[0].kind, PrincipalKind::User);
        assert!(principals[0].roles.contains("admin"));
    }
}
