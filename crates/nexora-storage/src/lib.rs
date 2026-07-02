//! Nexora Persistent Storage — PostgreSQL (primary) + SQLite (edge fallback).
//!
//! # Backends
//!
//! - **PostgreSQL** (default, primary): production-grade, supports concurrent
//!   reads/writes, full-text search, JSONB. Use this for any deployment that
//!   has more than one Core process or expects >100 RPS.
//! - **SQLite** (edge): embedded, zero-config, single-file. Use this for
//!   Tier-1 low-resource deployments (Part 10) where running a separate
//!   PostgreSQL server is not feasible.
//!
//! Both backends implement the same logical schema and the same store
//! interfaces, so swapping is a config change.
//!
//! # Stores
//!
//! | Store | PostgreSQL | SQLite |
//! |-------|:---------:|:------:|
//! | Users | ✅ `pg::PgUserStore` | ✅ `SqliteUserStore` |
//! | Sessions | ✅ `pg::PgSessionStore` | ❌ (in-memory only) |
//! | Events | ✅ `pg::PgEventStore` | ✅ `SqliteEventStore` |
//! | Packages | ✅ `pg::PgPackageStore` | ✅ `SqlitePackageStore` |
//! | Billing | ✅ `pg::PgBillingStore` | ✅ `SqliteBillingStore` |
//! | Audit | ✅ `pg::PgAuditStore` | ❌ (in-memory only) |
//! | Secrets | ✅ `pg::PgSecretStore` | ❌ (in-memory only) |

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod billing;
pub mod events;
pub mod packages;
pub mod schema;
pub mod users;

#[cfg(feature = "postgres")]
pub mod pg;

pub use billing::SqliteBillingStore;
pub use events::SqliteEventStore;
pub use packages::SqlitePackageStore;
pub use schema::{init_schema, StorageError};
pub use users::SqliteUserStore;

use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;

/// A thread-safe SQLite connection pool. In v0.1 we use a single connection
/// wrapped in a Mutex (SQLite serializes writes anyway). For higher throughput,
/// v0.2 can use a connection pool.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

impl Database {
    /// Open a database at the given path. Creates the file if it doesn't exist.
    /// Initializes the schema if needed.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        init_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        init_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Execute a closure with a locked connection.
    pub fn with_conn<F, T>(&self, f: F) -> Result<T, StorageError>
    where
        F: FnOnce(&mut Connection) -> Result<T, StorageError>,
    {
        let mut conn = self.conn.lock();
        f(&mut conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_works() {
        let db = Database::open_in_memory().unwrap();
        // Tables should exist.
        db.with_conn(|conn| {
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table'", [], |row| row.get(0))
                .unwrap();
            assert!(count >= 3); // users, events, packages at minimum
            Ok(())
        })
        .unwrap();
    }
}
