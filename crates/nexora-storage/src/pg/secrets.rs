//! PostgreSQL secrets store — stores encrypted secrets at rest.
//!
//! The actual encryption is performed by `nexora-core::secrets`; this module
//! only persists the (ciphertext, nonce) pair keyed by name.

use crate::pg::{PgError, PgPool};
use time::OffsetDateTime;

/// A stored secret.
#[derive(Debug, Clone)]
pub struct StoredSecret {
    pub key: String,
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub created_at: i64,
    pub rotated_at: Option<i64>,
}

/// PostgreSQL secrets store.
pub struct PgSecretStore {
    pool: PgPool,
}

impl PgSecretStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert or replace a secret.
    pub async fn upsert(&self, key: &str, ciphertext: &[u8], nonce: &[u8]) -> Result<(), PgError> {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        self.pool.execute(
            r#"INSERT INTO secrets (key, ciphertext, nonce, created_at, rotated_at)
               VALUES ($1, $2, $3, $4, NULL)
               ON CONFLICT (key) DO UPDATE SET
                   ciphertext = EXCLUDED.ciphertext,
                   nonce = EXCLUDED.nonce,
                   rotated_at = EXCLUDED.created_at"#,
            &[
                &key as &(dyn postgres_types::ToSql + Sync),
                &ciphertext.to_vec(),
                &nonce.to_vec(),
                &now,
            ],
        ).await?;
        Ok(())
    }

    /// Fetch a secret.
    pub async fn get(&self, key: &str) -> Result<Option<StoredSecret>, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_opt(
            "SELECT key, ciphertext, nonce, created_at, rotated_at FROM secrets WHERE key = $1",
            &[&key],
        ).await?;
        match row {
            Some(r) => Ok(Some(StoredSecret {
                key: r.get(0),
                ciphertext: r.get(1),
                nonce: r.get(2),
                created_at: r.get(3),
                rotated_at: r.get(4),
            })),
            None => Ok(None),
        }
    }

    /// Delete a secret.
    pub async fn delete(&self, key: &str) -> Result<u64, PgError> {
        self.pool.execute("DELETE FROM secrets WHERE key = $1", &[&key]).await
    }

    /// List all secret keys (without revealing ciphertext).
    pub async fn list_keys(&self) -> Result<Vec<String>, PgError> {
        let conn = self.pool.get_conn().await?;
        let rows = conn.query("SELECT key FROM secrets ORDER BY key", &[]).await?;
        Ok(rows.iter().map(|r| r.get(0)).collect())
    }

    /// Count secrets.
    pub async fn count(&self) -> Result<i64, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM secrets", &[]).await?;
        Ok(row.get(0))
    }
}
