//! PostgreSQL backend — for Tier 2/3 deployments.
//!
//! See Nexora Engineering Specification, Part 10 (DEPLOYMENT MODEL):
//! - Tier 1 (Edge): SQLite (embedded, single-node)
//! - Tier 2 (Standard): PostgreSQL (lightweight cluster)
//! - Tier 3 (Global): PostgreSQL (multi-region, replicated)
//!
//! This module provides PostgreSQL-backed stores that implement the same
//! public API as the SQLite stores. Enable with the `postgres` feature flag.
//!
//! # Configuration
//!
//! Set the `DATABASE_URL` environment variable:
//! ```text
//! DATABASE_URL=postgresql://user:pass@localhost:5432/nexora
//! ```
//!
//! # Feature Flags
//!
//! ```toml
//! [dependencies]
//! nexora-storage = { features = ["postgres"] }
//! ```

#![cfg(feature = "postgres")]

use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use std::fmt;
use std::str::FromStr;
use tokio_postgres::NoTls;

/// Error from PostgreSQL operations.
#[derive(Debug, thiserror::Error)]
pub enum PgError {
    /// PostgreSQL error.
    #[error("postgres: {0}")]
    Postgres(#[from] tokio_postgres::Error),
    /// Connection pool error.
    #[error("pool: {0}")]
    Pool(#[from] bb8::RunError<tokio_postgres::Error>),
    /// Configuration error.
    #[error("config: {0}")]
    Config(String),
    /// Serialization error.
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

/// PostgreSQL connection pool wrapper.
#[derive(Clone)]
pub struct PgDatabase {
    pool: Pool<PostgresConnectionManager<NoTls>>,
}

impl fmt::Debug for PgDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PgDatabase")
            .field("pool_state", &self.pool.state())
            .finish()
    }
}

impl PgDatabase {
    /// Connect to a PostgreSQL database using the given URL.
    ///
    /// ```rust,no_run
    /// # use nexora_storage::pg::PgDatabase;
    /// # async fn example() {
    /// let db = PgDatabase::connect("postgresql://user:pass@localhost:5432/nexora").await.unwrap();
    /// # }
    /// ```
    pub async fn connect(url: &str) -> Result<Self, PgError> {
        let manager = PostgresConnectionManager::new_from_stringlike(url, NoTls)
            .map_err(|e| PgError::Config(e.to_string()))?;
        let pool = Pool::builder()
            .max_size(10)
            .build(manager)
            .await?;
        Ok(Self { pool })
    }

    /// Connect from the `DATABASE_URL` environment variable.
    pub async fn from_env() -> Result<Self, PgError> {
        let url = std::env::var("DATABASE_URL")
            .map_err(|_| PgError::Config("DATABASE_URL not set".into()))?;
        Self::connect(&url).await
    }

    /// Initialize the schema (creates tables if they don't exist).
    pub async fn init_schema(&self) -> Result<(), PgError> {
        let conn = self.pool.get().await?;
        let statements = [
            "CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, username TEXT UNIQUE NOT NULL, password_hash TEXT NOT NULL, email TEXT, roles TEXT NOT NULL DEFAULT '[]', created_at BIGINT NOT NULL, last_login BIGINT, active INTEGER NOT NULL DEFAULT 1)",
            "CREATE TABLE IF NOT EXISTS events (id BIGSERIAL PRIMARY KEY, name TEXT NOT NULL, payload_text TEXT, payload_bytes BYTEA, timestamp BIGINT NOT NULL)",
            "CREATE INDEX IF NOT EXISTS idx_events_name ON events(name)",
            "CREATE INDEX IF NOT EXISTS idx_events_ts ON events(timestamp)",
            "CREATE TABLE IF NOT EXISTS packages (id TEXT NOT NULL, version TEXT NOT NULL, manifest_json TEXT NOT NULL, integrity_hash TEXT NOT NULL, published_at BIGINT NOT NULL, install_count BIGINT NOT NULL DEFAULT 0, active_install_count BIGINT NOT NULL DEFAULT 0, installed INTEGER NOT NULL DEFAULT 0, trust_json TEXT NOT NULL DEFAULT '{}', PRIMARY KEY (id, version))",
            "CREATE TABLE IF NOT EXISTS invoices (id TEXT PRIMARY KEY, customer_id TEXT NOT NULL, customer_name TEXT NOT NULL, items_json TEXT NOT NULL, total_minor BIGINT NOT NULL, currency TEXT NOT NULL, status TEXT NOT NULL, created_at BIGINT NOT NULL, due_at BIGINT NOT NULL, paid_at BIGINT, subscription_id TEXT, payment_ids_json TEXT NOT NULL DEFAULT '[]')",
            "CREATE TABLE IF NOT EXISTS payments (id TEXT PRIMARY KEY, invoice_id TEXT NOT NULL, customer_id TEXT NOT NULL, amount_minor BIGINT NOT NULL, currency TEXT NOT NULL, status TEXT NOT NULL, method TEXT NOT NULL, created_at BIGINT NOT NULL, processed_at BIGINT, failure_reason TEXT)",
            "CREATE TABLE IF NOT EXISTS subscriptions (id TEXT PRIMARY KEY, customer_id TEXT NOT NULL, package_id TEXT NOT NULL, price_minor BIGINT NOT NULL, currency TEXT NOT NULL, period_seconds BIGINT NOT NULL, status TEXT NOT NULL, started_at BIGINT NOT NULL, current_period_end BIGINT NOT NULL, cancelled_at BIGINT)",
            "CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        ];
        for stmt in &statements {
            conn.execute(*stmt, &[]).await?;
        }
        Ok(())
    }

    /// Get a connection from the pool.
    pub async fn conn(&self) -> Result<bb8::PooledConnection<'_, PostgresConnectionManager<NoTls>>, PgError> {
        Ok(self.pool.get().await?)
    }

    /// Pool state (for observability).
    pub fn pool_state(&self) -> bb8::State {
        self.pool.state()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_error_display() {
        let e = PgError::Config("test".into());
        assert!(e.to_string().contains("test"));
    }
}
