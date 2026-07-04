//! PostgreSQL backend — the primary database for Nexora.
//!
//! This module provides 7 native PostgreSQL stores, all sharing a single
//! `bb8` connection pool:
//!
//! - [`users::PgUserStore`] — user accounts
//! - [`sessions::PgSessionStore`] — auth tokens
//! - [`events::PgEventStore`] — event-sourcing log
//! - [`packages::PgPackageStore`] — marketplace catalog
//! - [`billing::PgBillingStore`] — invoices, payments, subscriptions
//! - [`audit::PgAuditStore`] — audit log
//! - [`secrets::PgSecretStore`] — encrypted secrets at rest
//!
//! # Why PostgreSQL
//!
//! Per user requirement: PostgreSQL is the primary database. SQLite remains
//! as an optional edge fallback for Tier-1 low-resource deployments (Part 10).
//!
//! # Schema
//!
//! The schema is created automatically on first connect via `refinery`
//! migrations in [`migrations`].

pub mod audit;
pub mod billing;
pub mod events;
pub mod migrations;
pub mod packages;
pub mod pool;
pub mod secrets;
pub mod sessions;
pub mod users;

pub use audit::{AuditEntry, PgAuditStore};
pub use billing::PgBillingStore;
pub use events::PgEventStore;
pub use packages::{PgPackageRow, PgPackageStore};
pub use pool::{PgError, PgPool};
pub use secrets::{PgSecretStore, StoredSecret};
pub use sessions::{PgSession, PgSessionStore};
pub use users::{PgUserRecord, PgUserStore};
