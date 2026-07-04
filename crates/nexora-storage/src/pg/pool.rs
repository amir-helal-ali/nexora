//! PostgreSQL connection pool — the primary database for Nexora.
//!
//! Uses `bb8` for connection pooling and `tokio-postgres` for the wire
//! protocol. All stores in this module share one pool.
//!
//! # Example
//!
//! ```no_run
//! # use nexora_storage::pg::PgPool;
//! # #[tokio::main] async fn main() -> anyhow::Result<()> {
//! let pool = PgPool::connect("postgres://user:pass@localhost/nexora").await?;
//! pool.execute("SELECT 1", &[]).await?;
//! # Ok(()) }
//! ```

use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use std::time::Duration;
use tokio_postgres::NoTls;

/// A PostgreSQL connection pool. Cloning is cheap — the underlying pool is
/// reference-counted.
#[derive(Clone)]
pub struct PgPool {
    pool: Pool<PostgresConnectionManager<NoTls>>,
}

impl std::fmt::Debug for PgPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgPool")
            .field("state", &self.pool.state())
            .finish()
    }
}

impl PgPool {
    /// Connect to a PostgreSQL database. Applies pending migrations.
    pub async fn connect(database_url: &str) -> Result<Self, PgError> {
        Self::with_config(database_url, 8, Duration::from_secs(30)).await
    }

    /// Connect with a custom pool size and connection timeout.
    pub async fn with_config(
        database_url: &str,
        max_size: u32,
        timeout: Duration,
    ) -> Result<Self, PgError> {
        let manager = PostgresConnectionManager::new_from_stringlike(database_url, NoTls)?;
        let pool = Pool::builder()
            .max_size(max_size)
            .connection_timeout(timeout)
            .build(manager)
            .await?;

        // Apply migrations synchronously on the first connection.
        let pool_self = Self { pool };
        crate::pg::migrations::apply_schema(&pool_self).await?;

        Ok(pool_self)
    }

    /// Execute a statement that returns no rows.
    pub async fn execute(
        &self,
        sql: &str,
        params: &[&(dyn postgres_types::ToSql + Sync)],
    ) -> Result<u64, PgError> {
        let conn = self.pool.get().await?;
        let n = conn.execute(sql, params).await?;
        Ok(n)
    }

    /// Get a raw pooled connection (for advanced use cases).
    pub async fn get_conn(
        &self,
    ) -> Result<bb8::PooledConnection<'_, PostgresConnectionManager<NoTls>>, PgError>
    {
        Ok(self.pool.get().await?)
    }

    /// Pool statistics for diagnostics.
    pub fn pool_state(&self) -> bb8::State {
        self.pool.state()
    }
}

/// PostgreSQL-related errors.
#[derive(Debug, thiserror::Error)]
pub enum PgError {
    #[error("postgres connection error: {0}")]
    Connection(String),

    #[error("postgres query error: {0}")]
    Query(String),

    #[error("migration failed: {0}")]
    Migration(String),

    #[error("pool error: {0}")]
    Pool(String),

    #[error("serialization error: {0}")]
    Serde(String),

    #[error("not found: {0}")]
    NotFound(String),
}

impl From<bb8::RunError<tokio_postgres::Error>> for PgError {
    fn from(e: bb8::RunError<tokio_postgres::Error>) -> Self {
        match e {
            bb8::RunError::User(e) => PgError::Query(e.to_string()),
            bb8::RunError::TimedOut => PgError::Pool("connection pool timed out".into()),
        }
    }
}

impl From<tokio_postgres::Error> for PgError {
    fn from(e: tokio_postgres::Error) -> Self {
        PgError::Query(e.to_string())
    }
}

impl From<serde_json::Error> for PgError {
    fn from(e: serde_json::Error) -> Self {
        PgError::Serde(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This test only runs when the `NEXORA_PG_TEST_URL` environment variable
    /// is set (so CI without PostgreSQL can skip it).
    #[tokio::test]
    async fn pool_connects_to_real_db() {
        let url = match std::env::var("NEXORA_PG_TEST_URL") {
            Ok(u) => u,
            Err(_) => {
                eprintln!("skipping test: NEXORA_PG_TEST_URL not set");
                return;
            }
        };
        let pool = PgPool::connect(&url).await.expect("connect");
        let n = pool.execute("SELECT 1", &[]).await.expect("select");
        assert_eq!(n, 1);
    }
}
