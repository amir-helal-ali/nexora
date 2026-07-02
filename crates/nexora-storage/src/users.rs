//! SQLite-backed user store.
//!
//! Provides the same public API as `nexora_auth::UserStore` but persists
//! to SQLite. On startup, call `load_into()` to populate the in-memory store;
//! thereafter, every write operation writes through to SQLite.

use crate::{Database, StorageError};
use nexora_auth::users::{User, UserError, UserId};
use nexora_auth::password::HashedPassword;
use nexora_core::permissions::{PermissionEngine, Principal, PrincipalKind};
use nexora_core::events::EventPayload;
use std::sync::Arc;
use time::OffsetDateTime;

/// SQLite-backed user store. Wraps the in-memory `nexora_auth::UserStore`
/// and writes through to SQLite on every mutation.
pub struct SqliteUserStore {
    db: Database,
    permission_engine: Option<Arc<PermissionEngine>>,
    event_bus: Option<Arc<nexora_core::EventBus>>,
}

impl std::fmt::Debug for SqliteUserStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteUserStore")
            .field("db", &self.db)
            .finish_non_exhaustive()
    }
}

impl SqliteUserStore {
    /// Construct a new SQLite-backed user store.
    pub fn new(db: Database) -> Self {
        Self {
            db,
            permission_engine: None,
            event_bus: None,
        }
    }

    /// Attach a Permission Engine for auto-principal registration.
    pub fn with_permission_engine(mut self, engine: Arc<PermissionEngine>) -> Self {
        self.permission_engine = Some(engine);
        self
    }

    /// Attach an Event Bus for auto-event publishing.
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Load all users from SQLite into memory (call on startup).
    /// Also re-registers principals in the Permission Engine.
    pub fn load_into(&self, mem: &nexora_auth::UserStore) -> Result<usize, StorageError> {
        let count = self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, username, password_hash, email, roles, created_at, last_login, active
                 FROM users",
            )?;
            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let username: String = row.get(1)?;
                let password_hash_str: String = row.get(2)?;
                let email: Option<String> = row.get(3)?;
                let roles_json: String = row.get(4)?;
                let created_at: i64 = row.get(5)?;
                let last_login: Option<i64> = row.get(6)?;
                let active: i64 = row.get(7)?;
                Ok((id, username, password_hash_str, email, roles_json, created_at, last_login, active))
            })?;

            let mut count = 0;
            for row_result in rows {
                let (id, username, password_hash_str, email, roles_json, created_at, last_login, active) =
                    row_result?;
                let password_hash = HashedPassword::from_str(&password_hash_str)
                    .map_err(|e| StorageError::Other(e.to_string()))?;
                let roles: Vec<String> = serde_json::from_str(&roles_json)?;

                // Insert directly into the in-memory store using insert_raw
                // (bypasses password hashing — the hash is already in the DB).
                let user = User {
                    id: id.clone(),
                    username: username.clone(),
                    password_hash: Some(password_hash),
                    email,
                    roles: roles.clone(),
                    created_at,
                    last_login,
                    active: active != 0,
                };

                mem.insert_raw(user);

                // Re-register principal in the Permission Engine.
                if let Some(engine) = &self.permission_engine {
                    engine.register_principal(Principal {
                        id: id.clone(),
                        name: username.clone(),
                        kind: PrincipalKind::User,
                        roles: roles.into_iter().collect(),
                    });
                }

                count += 1;
            }
            Ok(count)
        })?;
        Ok(count)
    }

    /// Create a new user. Writes to SQLite + in-memory store.
    pub fn create(
        &self,
        mem: &nexora_auth::UserStore,
        username: impl Into<String>,
        password: &str,
        email: Option<String>,
        roles: Vec<String>,
    ) -> Result<User, UserError> {
        // Use the in-memory store's create() (which hashes the password).
        let user = mem.create(username, password, email, roles.clone())?;

        // Write through to SQLite.
        let password_hash_str = user
            .password_hash
            .as_ref()
            .map(|h| h.as_str().to_string())
            .unwrap_or_default();
        let roles_json = serde_json::to_string(&user.roles)
            .map_err(|e| UserError::Password(nexora_auth::PasswordError::InvalidHash(e.to_string())))?;

        self.db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO users (id, username, password_hash, email, roles, created_at, last_login, active)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        user.id,
                        user.username,
                        password_hash_str,
                        user.email,
                        roles_json,
                        user.created_at,
                        user.last_login,
                        if user.active { 1 } else { 0 },
                    ],
                )?;
                Ok(())
            })
            .map_err(|e| UserError::Password(nexora_auth::PasswordError::InvalidHash(e.to_string())))?;

        Ok(user)
    }

    /// Record a login. Updates SQLite + in-memory.
    pub fn record_login(&self, mem: &nexora_auth::UserStore, id: &str) -> Option<User> {
        let user = mem.record_login(id)?;
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let _ = self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE users SET last_login = ?1 WHERE id = ?2",
                rusqlite::params![now, id],
            )?;
            Ok(())
        });
        Some(user)
    }

    /// Delete a user. Removes from SQLite + in-memory.
    pub fn delete(&self, mem: &nexora_auth::UserStore, id: &str) -> Result<(), UserError> {
        mem.delete(id)?;
        let _ = self.db.with_conn(|conn| {
            conn.execute("DELETE FROM users WHERE id = ?1", rusqlite::params![id])?;
            Ok(())
        });
        Ok(())
    }

    /// Number of users in the database.
    pub fn count(&self) -> Result<i64, StorageError> {
        self.db.with_conn(|conn| {
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
            Ok(count)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_auth::UserStore;

    fn setup() -> (SqliteUserStore, UserStore) {
        let db = Database::open_in_memory().unwrap();
        let engine = Arc::new(PermissionEngine::new());
        let bus = Arc::new(nexora_core::EventBus::new());
        let sql = SqliteUserStore::new(db)
            .with_permission_engine(engine.clone())
            .with_event_bus(bus.clone());
        let mem = UserStore::new()
            .with_permission_engine(engine)
            .with_event_bus(bus);
        (sql, mem)
    }

    #[test]
    fn create_and_count() {
        let (sql, mem) = setup();
        assert_eq!(sql.count().unwrap(), 0);
        sql.create(&mem, "alice", "hunter2", None, vec!["admin".into()]).unwrap();
        assert_eq!(sql.count().unwrap(), 1);
        assert_eq!(mem.user_count(), 1);
    }

    #[test]
    fn load_into_restores_users() {
        // Create a fresh in-memory DB with one user.
        let (sql1, mem1) = setup();
        sql1.create(&mem1, "alice", "hunter2", Some("alice@test".into()), vec!["admin".into()]).unwrap();
        sql1.create(&mem1, "bob", "secret", None, vec!["viewer".into()]).unwrap();

        // Simulate a restart: open a new in-memory store + load from SQLite.
        // Since we used open_in_memory(), we need to re-use the same DB.
        // For this test, we just verify load_into works on the same DB.
        let (_, mem2) = setup();
        let loaded = sql1.load_into(&mem2).unwrap();
        assert_eq!(loaded, 2);
        assert_eq!(mem2.user_count(), 2);
        assert!(mem2.get_by_username("alice").is_some());
        assert!(mem2.get_by_username("bob").is_some());
    }

    #[test]
    fn delete_removes_from_both() {
        let (sql, mem) = setup();
        let user = sql.create(&mem, "alice", "pw", None, vec![]).unwrap();
        assert_eq!(sql.count().unwrap(), 1);
        sql.delete(&mem, &user.id).unwrap();
        assert_eq!(sql.count().unwrap(), 0);
        assert_eq!(mem.user_count(), 0);
    }

    #[test]
    fn record_login_updates_sqlite() {
        let (sql, mem) = setup();
        let user = sql.create(&mem, "alice", "pw", None, vec![]).unwrap();
        assert!(user.last_login.is_none());
        let updated = sql.record_login(&mem, &user.id).unwrap();
        assert!(updated.last_login.is_some());
        // Verify the DB has the login timestamp.
        let db_login: Option<i64> = sql
            .db
            .with_conn(|conn| {
                let v: Option<i64> = conn
                    .query_row("SELECT last_login FROM users WHERE id = ?1", rusqlite::params![user.id], |row| row.get(0))?;
                Ok(v)
            })
            .unwrap();
        assert!(db_login.is_some());
    }
}
