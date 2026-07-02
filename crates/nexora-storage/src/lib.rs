//! Nexora Persistent Storage — SQLite backend.
//!
//! Provides durable storage for:
//! - Users (with Argon2 password hashes)
//! - Events (the source of truth — event sourcing)
//! - Packages (marketplace catalog)
//!
//! # Design
//!
//! Uses a **write-through cache** pattern:
//! - SQLite is the durability layer (writes go to disk)
//! - In-memory stores remain the primary read path (fast)
//! - On startup, state is loaded from SQLite into memory
//!
//! This gives us durability without sacrificing read performance.
//!
//! # Tier-1 (Edge) Appropriate
//!
//! SQLite is embedded (no external server), making it ideal for Tier-1
//! low-resource deployments (Part 10). In Tier 2/3, swap with PostgreSQL
//! using the same trait interface.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

#[cfg(feature = "sqlite")]
pub mod billing;
#[cfg(feature = "sqlite")]
pub mod events;
#[cfg(feature = "sqlite")]
pub mod packages;
#[cfg(feature = "sqlite")]
pub mod schema;
#[cfg(feature = "sqlite")]
pub mod users;

#[cfg(feature = "postgres")]
pub mod pg;

#[cfg(feature = "sqlite")]
pub use billing::SqliteBillingStore;
#[cfg(feature = "sqlite")]
pub use events::SqliteEventStore;
#[cfg(feature = "sqlite")]
pub use packages::SqlitePackageStore;
#[cfg(feature = "sqlite")]
pub use schema::{init_schema, StorageError};
#[cfg(feature = "sqlite")]
pub use users::SqliteUserStore;

#[cfg(feature = "postgres")]
pub use pg::{PgDatabase, PgError};

#[cfg(feature = "sqlite")]
use parking_lot::Mutex;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
#[cfg(feature = "sqlite")]
use std::path::Path;
#[cfg(feature = "sqlite")]
use std::sync::Arc;

/// A thread-safe SQLite connection pool (Tier-1 backend).
#[cfg(feature = "sqlite")]
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

#[cfg(feature = "sqlite")]
impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

#[cfg(feature = "sqlite")]
impl Database {
    /// Open a database at the given path.
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
