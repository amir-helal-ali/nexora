//! PostgreSQL session store — persists auth tokens.

use crate::pg::{PgError, PgPool};
use time::OffsetDateTime;

/// A stored session.
#[derive(Debug, Clone)]
pub struct PgSession {
    pub id: String,
    pub user_id: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub token_version: i64,
    pub revoked: bool,
}

/// PostgreSQL session store.
pub struct PgSessionStore {
    pool: PgPool,
}

impl PgSessionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a new session.
    pub async fn insert(&self, s: &PgSession) -> Result<(), PgError> {
        self.pool.execute(
            r#"INSERT INTO sessions (id, user_id, issued_at, expires_at, token_version, revoked)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT (id) DO UPDATE SET
                   revoked = EXCLUDED.revoked,
                   token_version = EXCLUDED.token_version"#,
            &[
                &s.id as &(dyn postgres_types::ToSql + Sync),
                &s.user_id,
                &s.issued_at,
                &s.expires_at,
                &s.token_version,
                &s.revoked,
            ],
        ).await?;
        Ok(())
    }

    /// Fetch a session by ID.
    pub async fn get(&self, id: &str) -> Result<Option<PgSession>, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_opt(
            "SELECT id, user_id, issued_at, expires_at, token_version, revoked FROM sessions WHERE id = $1",
            &[&id],
        ).await?;
        match row {
            Some(r) => Ok(Some(PgSession {
                id: r.get(0),
                user_id: r.get(1),
                issued_at: r.get(2),
                expires_at: r.get(3),
                token_version: r.get(4),
                revoked: r.get(5),
            })),
            None => Ok(None),
        }
    }

    /// Revoke a session.
    pub async fn revoke(&self, id: &str) -> Result<u64, PgError> {
        self.pool.execute(
            "UPDATE sessions SET revoked = TRUE WHERE id = $1",
            &[&id],
        ).await
    }

    /// Revoke all sessions for a user (e.g. on logout-everywhere).
    pub async fn revoke_all_for_user(&self, user_id: &str) -> Result<u64, PgError> {
        self.pool.execute(
            "UPDATE sessions SET revoked = TRUE WHERE user_id = $1",
            &[&user_id],
        ).await
    }

    /// Delete expired sessions.
    pub async fn purge_expired(&self) -> Result<u64, PgError> {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        self.pool.execute(
            "DELETE FROM sessions WHERE expires_at < $1",
            &[&now],
        ).await
    }

    /// Count active sessions.
    pub async fn active_count(&self) -> Result<i64, PgError> {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let conn = self.pool.get_conn().await?;
        let row = conn.query_one(
            "SELECT COUNT(*) FROM sessions WHERE revoked = FALSE AND expires_at >= $1",
            &[&now],
        ).await?;
        Ok(row.get(0))
    }
}
