//! Database schema initialization.
//!
//! Creates tables for users, events, packages, and package versions.

use rusqlite::Connection;
use thiserror::Error;

/// Error from storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    /// SQLite error.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Serialization error.
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    /// Row not found.
    #[error("not found: {0}")]
    NotFound(String),
    /// Duplicate entry.
    #[error("duplicate: {0}")]
    Duplicate(String),
    /// Generic error.
    #[error("{0}")]
    Other(String),
}

/// Initialize the database schema. Creates tables if they don't exist.
/// Idempotent — safe to call on every startup.
pub fn init_schema(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        "
        -- Users table
        CREATE TABLE IF NOT EXISTS users (
            id              TEXT PRIMARY KEY,
            username        TEXT UNIQUE NOT NULL,
            password_hash   TEXT NOT NULL,
            email           TEXT,
            roles           TEXT NOT NULL DEFAULT '[]',
            created_at      INTEGER NOT NULL,
            last_login      INTEGER,
            active          INTEGER NOT NULL DEFAULT 1
        );

        -- Events table (event sourcing — source of truth)
        CREATE TABLE IF NOT EXISTS events (
            id              INTEGER PRIMARY KEY,
            name            TEXT NOT NULL,
            payload_text    TEXT,
            payload_bytes   BLOB,
            timestamp       INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_events_name ON events(name);
        CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);

        -- Packages table (one row per published version)
        CREATE TABLE IF NOT EXISTS packages (
            id                  TEXT NOT NULL,
            version             TEXT NOT NULL,
            manifest_json       TEXT NOT NULL,
            integrity_hash      TEXT NOT NULL,
            published_at        INTEGER NOT NULL,
            install_count       INTEGER NOT NULL DEFAULT 0,
            active_install_count INTEGER NOT NULL DEFAULT 0,
            installed           INTEGER NOT NULL DEFAULT 0,
            trust_json          TEXT NOT NULL DEFAULT '{}',
            PRIMARY KEY (id, version)
        );
        CREATE INDEX IF NOT EXISTS idx_packages_id ON packages(id);

        -- Key-value store for metadata (e.g. next event ID)
        CREATE TABLE IF NOT EXISTS kv (
            key     TEXT PRIMARY KEY,
            value   TEXT NOT NULL
        );

        -- Invoices table
        CREATE TABLE IF NOT EXISTS invoices (
            id                  TEXT PRIMARY KEY,
            customer_id         TEXT NOT NULL,
            customer_name       TEXT NOT NULL,
            items_json          TEXT NOT NULL,
            total_minor         INTEGER NOT NULL,
            currency            TEXT NOT NULL,
            status              TEXT NOT NULL,
            created_at          INTEGER NOT NULL,
            due_at              INTEGER NOT NULL,
            paid_at             INTEGER,
            subscription_id     TEXT,
            payment_ids_json    TEXT NOT NULL DEFAULT '[]'
        );
        CREATE INDEX IF NOT EXISTS idx_invoices_customer ON invoices(customer_id);

        -- Payments table
        CREATE TABLE IF NOT EXISTS payments (
            id              TEXT PRIMARY KEY,
            invoice_id      TEXT NOT NULL,
            customer_id     TEXT NOT NULL,
            amount_minor    INTEGER NOT NULL,
            currency        TEXT NOT NULL,
            status          TEXT NOT NULL,
            method          TEXT NOT NULL,
            created_at      INTEGER NOT NULL,
            processed_at    INTEGER,
            failure_reason  TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_payments_invoice ON payments(invoice_id);
        CREATE INDEX IF NOT EXISTS idx_payments_customer ON payments(customer_id);

        -- Subscriptions table
        CREATE TABLE IF NOT EXISTS subscriptions (
            id                      TEXT PRIMARY KEY,
            customer_id             TEXT NOT NULL,
            package_id              TEXT NOT NULL,
            price_minor             INTEGER NOT NULL,
            currency                TEXT NOT NULL,
            period_seconds          INTEGER NOT NULL,
            status                  TEXT NOT NULL,
            started_at              INTEGER NOT NULL,
            current_period_end      INTEGER NOT NULL,
            cancelled_at            INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_subscriptions_customer ON subscriptions(customer_id);

        -- Workflows table
        CREATE TABLE IF NOT EXISTS workflows (
            id                  TEXT PRIMARY KEY,
            name                TEXT NOT NULL,
            description         TEXT NOT NULL DEFAULT '',
            trigger_json        TEXT NOT NULL,
            steps_json          TEXT NOT NULL,
            enabled             INTEGER NOT NULL DEFAULT 1,
            created_at          INTEGER NOT NULL,
            execution_count     INTEGER NOT NULL DEFAULT 0
        );

        -- Workflow executions table
        CREATE TABLE IF NOT EXISTS workflow_executions (
            id                  TEXT PRIMARY KEY,
            workflow_id         TEXT NOT NULL,
            trigger_event       TEXT,
            trigger_payload     TEXT,
            status              TEXT NOT NULL,
            step_results_json   TEXT NOT NULL DEFAULT '[]',
            started_at          INTEGER NOT NULL,
            finished_at         INTEGER,
            error               TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_wf_exec_workflow ON workflow_executions(workflow_id);

        -- Notifications table
        CREATE TABLE IF NOT EXISTS notifications (
            id                  TEXT PRIMARY KEY,
            user_id             TEXT NOT NULL,
            title               TEXT NOT NULL,
            body                TEXT NOT NULL,
            severity            TEXT NOT NULL DEFAULT 'info',
            read                INTEGER NOT NULL DEFAULT 0,
            created_at          INTEGER NOT NULL,
            link                TEXT,
            icon                TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_notif_user ON notifications(user_id);
        CREATE INDEX IF NOT EXISTS idx_notif_unread ON notifications(user_id, read);

        -- Organizations table
        CREATE TABLE IF NOT EXISTS organizations (
            id                  TEXT PRIMARY KEY,
            name                TEXT NOT NULL,
            slug                TEXT UNIQUE NOT NULL,
            tier                TEXT NOT NULL,
            owner_id            TEXT NOT NULL,
            description         TEXT NOT NULL DEFAULT '',
            active              INTEGER NOT NULL DEFAULT 1,
            created_at          INTEGER NOT NULL,
            max_members         INTEGER NOT NULL
        );

        -- Organization memberships table
        CREATE TABLE IF NOT EXISTS org_memberships (
            org_id              TEXT NOT NULL,
            user_id             TEXT NOT NULL,
            role                TEXT NOT NULL,
            joined_at           INTEGER NOT NULL,
            PRIMARY KEY (org_id, user_id)
        );
        CREATE INDEX IF NOT EXISTS idx_memberships_user ON org_memberships(user_id);

        -- Teams table
        CREATE TABLE IF NOT EXISTS teams (
            id                  TEXT PRIMARY KEY,
            org_id              TEXT NOT NULL,
            name                TEXT NOT NULL,
            description         TEXT NOT NULL DEFAULT '',
            member_ids_json     TEXT NOT NULL DEFAULT '[]',
            created_at          INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_teams_org ON teams(org_id);
        ",
    )?;
    Ok(())
}
