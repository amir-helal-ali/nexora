//! PostgreSQL audit log store.

use crate::pg::{PgError, PgPool};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One row in the audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub actor: String,
    pub action: String,
    pub target: Option<String>,
    pub metadata: HashMap<String, String>,
    pub occurred_at: i64,
}

/// PostgreSQL audit log store.
pub struct PgAuditStore {
    pool: PgPool,
}

impl PgAuditStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Append an audit entry.
    pub async fn append(&self, entry: &AuditEntry) -> Result<i64, PgError> {
        let meta_json = serde_json::to_value(&entry.metadata)?;
        let conn = self.pool.get_conn().await?;
        let row = conn.query_one(
            "INSERT INTO audit_log (actor, action, target, metadata, occurred_at)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id",
            &[
                &entry.actor as &(dyn postgres_types::ToSql + Sync),
                &entry.action,
                &entry.target,
                &meta_json,
                &entry.occurred_at,
            ],
        ).await?;
        Ok(row.get(0))
    }

    /// List audit entries for an actor.
    pub async fn list_for_actor(&self, actor: &str, limit: i64) -> Result<Vec<AuditEntry>, PgError> {
        let conn = self.pool.get_conn().await?;
        let rows = conn.query(
            "SELECT id, actor, action, target, metadata, occurred_at
             FROM audit_log WHERE actor = $1
             ORDER BY occurred_at DESC LIMIT $2",
            &[&actor, &limit],
        ).await?;
        rows.iter().map(Self::row_to_entry).collect()
    }

    /// Count audit entries.
    pub async fn count(&self) -> Result<i64, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM audit_log", &[]).await?;
        Ok(row.get(0))
    }

    fn row_to_entry(row: &tokio_postgres::Row) -> Result<AuditEntry, PgError> {
        let meta_json: serde_json::Value = row.get(4);
        let metadata: HashMap<String, String> = serde_json::from_value(meta_json)?;
        Ok(AuditEntry {
            id: row.get(0),
            actor: row.get(1),
            action: row.get(2),
            target: row.get(3),
            metadata,
            occurred_at: row.get(5),
        })
    }
}
