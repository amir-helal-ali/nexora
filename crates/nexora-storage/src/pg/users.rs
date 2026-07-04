//! PostgreSQL-native user store.

use crate::pg::{PgError, PgPool};
use nexora_auth::password::{HashedPassword, PasswordError};
use nexora_auth::users::{User, UserId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// A serializable user record for PostgreSQL. Mirrors `nexora_auth::users::User`
/// but with `password_hash` as a string (PHC string format) so it survives
/// the round-trip through the JSONB column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgUserRecord {
    pub id: UserId,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub display_name: String,
    pub roles: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub disabled: bool,
}

impl PgUserRecord {
    /// Convert from a `User` (using current time as `updated_at`).
    pub fn from_user(user: &User, password_hash: &HashedPassword) -> Self {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        Self {
            id: user.id.clone(),
            username: user.username.clone(),
            email: user.email.clone(),
            password_hash: password_hash.as_str().to_string(),
            display_name: user.username.clone(),
            roles: user.roles.clone(),
            created_at: user.created_at,
            updated_at: now,
            disabled: !user.active,
        }
    }

    /// Convert to a `User`. Returns an error if the password hash is malformed.
    pub fn to_user(&self) -> Result<User, PasswordError> {
        let hash = HashedPassword::from_str(&self.password_hash)?;
        Ok(User {
            id: self.id.clone(),
            username: self.username.clone(),
            password_hash: Some(hash),
            email: self.email.clone(),
            roles: self.roles.clone(),
            created_at: self.created_at,
            last_login: None,
            active: !self.disabled,
        })
    }
}

/// PostgreSQL user store.
pub struct PgUserStore {
    pool: PgPool,
}

impl PgUserStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert or replace a user.
    pub async fn upsert(&self, rec: &PgUserRecord) -> Result<(), PgError> {
        let roles_json = serde_json::to_value(&rec.roles)?;
        self.pool.execute(
            r#"INSERT INTO users (id, username, email, password_hash, display_name,
                                    roles, created_at, updated_at, disabled)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               ON CONFLICT (id) DO UPDATE SET
                   username = EXCLUDED.username,
                   email = EXCLUDED.email,
                   password_hash = EXCLUDED.password_hash,
                   display_name = EXCLUDED.display_name,
                   roles = EXCLUDED.roles,
                   updated_at = EXCLUDED.updated_at,
                   disabled = EXCLUDED.disabled"#,
            &[
                &rec.id as &(dyn postgres_types::ToSql + Sync),
                &rec.username,
                &rec.email,
                &rec.password_hash,
                &rec.display_name,
                &roles_json,
                &rec.created_at,
                &rec.updated_at,
                &rec.disabled,
            ],
        ).await?;
        Ok(())
    }

    /// Fetch a user by ID.
    pub async fn get(&self, id: &str) -> Result<Option<PgUserRecord>, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_opt(
            "SELECT id, username, email, password_hash, display_name, roles, created_at, updated_at, disabled FROM users WHERE id = $1",
            &[&id],
        ).await?;

        match row {
            Some(r) => Ok(Some(Self::row_to_record(&r)?)),
            None => Ok(None),
        }
    }

    /// Fetch a user by username.
    pub async fn get_by_username(&self, username: &str) -> Result<Option<PgUserRecord>, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_opt(
            "SELECT id, username, email, password_hash, display_name, roles, created_at, updated_at, disabled FROM users WHERE username = $1",
            &[&username],
        ).await?;
        match row {
            Some(r) => Ok(Some(Self::row_to_record(&r)?)),
            None => Ok(None),
        }
    }

    /// Fetch a user by email.
    pub async fn get_by_email(&self, email: &str) -> Result<Option<PgUserRecord>, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_opt(
            "SELECT id, username, email, password_hash, display_name, roles, created_at, updated_at, disabled FROM users WHERE email = $1",
            &[&email],
        ).await?;
        match row {
            Some(r) => Ok(Some(Self::row_to_record(&r)?)),
            None => Ok(None),
        }
    }

    /// Count total users.
    pub async fn count(&self) -> Result<i64, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM users", &[]).await?;
        let n: i64 = row.get(0);
        Ok(n)
    }

    /// List all users (paginated).
    pub async fn list(&self, limit: i64, offset: i64) -> Result<Vec<PgUserRecord>, PgError> {
        let conn = self.pool.get_conn().await?;
        let rows = conn.query(
            "SELECT id, username, email, password_hash, display_name, roles, created_at, updated_at, disabled FROM users ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            &[&limit, &offset],
        ).await?;
        rows.iter().map(|r| Self::row_to_record(r)).collect()
    }

    /// Delete a user by ID.
    pub async fn delete(&self, id: &str) -> Result<u64, PgError> {
        self.pool.execute("DELETE FROM users WHERE id = $1", &[&id]).await
    }

    /// Mark a user as disabled.
    pub async fn disable(&self, id: &str) -> Result<(), PgError> {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        self.pool.execute(
            "UPDATE users SET disabled = TRUE, updated_at = $1 WHERE id = $2",
            &[&now, &id],
        ).await?;
        Ok(())
    }

    fn row_to_record(row: &tokio_postgres::Row) -> Result<PgUserRecord, PgError> {
        let roles_json: serde_json::Value = row.get(5);
        let roles: Vec<String> = serde_json::from_value(roles_json)?;
        Ok(PgUserRecord {
            id: row.get(0),
            username: row.get(1),
            email: row.get(2),
            password_hash: row.get(3),
            display_name: row.get(4),
            roles,
            created_at: row.get(6),
            updated_at: row.get(7),
            disabled: row.get(8),
        })
    }
}
