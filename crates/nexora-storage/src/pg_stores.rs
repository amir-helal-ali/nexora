//! PostgreSQL-native stores — full implementations for all data types.
//!
//! These stores use the same table schema as the SQLite versions but with
//! PostgreSQL-native types (BIGSERIAL, BYTEA, JSONB). They share a single
//! connection pool (`PgDatabase`).

#![cfg(feature = "postgres")]

use crate::pg::{PgDatabase, PgError};
use nexora_auth::password::HashedPassword;
use nexora_auth::users::{User, UserError};
use nexora_billing::types::{
    Invoice, InvoiceItem, InvoiceStatus, Payment, PaymentStatus, Subscription,
    SubscriptionStatus,
};
use nexora_core::events::{Event, EventId, EventPayload};
use nexora_marketplace::package::{Package, PackageManifest};
use nexora_marketplace::store::TrustScore;
use nexora_marketplace::version::Version;
use nexora_notifications::types::{Notification, NotificationSeverity};
use nexora_workflow::engine::{ExecutionStatus, StepResult, WorkflowExecution};
use nexora_workflow::types::{Workflow, WorkflowStep, WorkflowTrigger};
use std::sync::Arc;
use time::OffsetDateTime;

// ============================================================
// PgUserStore
// ============================================================

/// PostgreSQL-backed user store.
pub struct PgUserStore {
    db: PgDatabase,
}

impl PgUserStore {
    /// Construct.
    pub fn new(db: PgDatabase) -> Self { Self { db } }

    /// Create a user.
    pub async fn create(
        &self,
        user: &User,
        password_hash: &HashedPassword,
        roles: &[String],
    ) -> Result<(), PgError> {
        let conn = self.db.conn().await?;
        let roles_json = serde_json::to_string(roles)?;
        conn.execute(
            "INSERT INTO users (id, username, password_hash, email, roles, created_at, last_login, active)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            &[
                &user.id,
                &user.username,
                &password_hash.as_str(),
                &user.email,
                &roles_json,
                &user.created_at,
                &user.last_login,
                &(user.active as i32),
            ],
        ).await?;
        Ok(())
    }

    /// Count users.
    pub async fn count(&self) -> Result<i64, PgError> {
        let conn = self.db.conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM users", &[]).await?;
        Ok(row.get(0))
    }

    /// Record login.
    pub async fn record_login(&self, id: &str) -> Result<(), PgError> {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let conn = self.db.conn().await?;
        conn.execute(
            "UPDATE users SET last_login = $1 WHERE id = $2",
            &[&now, &id],
        ).await?;
        Ok(())
    }

    /// Delete user.
    pub async fn delete(&self, id: &str) -> Result<(), PgError> {
        let conn = self.db.conn().await?;
        conn.execute("DELETE FROM users WHERE id = $1", &[&id]).await?;
        Ok(())
    }
}

// ============================================================
// PgEventStore
// ============================================================

/// PostgreSQL-backed event store (source of truth).
pub struct PgEventStore {
    db: PgDatabase,
    bus: Arc<nexora_core::EventBus>,
}

impl PgEventStore {
    /// Construct.
    pub fn new(db: PgDatabase, bus: Arc<nexora_core::EventBus>) -> Self {
        Self { db, bus }
    }

    /// Publish an event (write to PostgreSQL + broadcast to in-memory bus).
    pub async fn publish(&self, name: &str, payload: EventPayload) -> EventId {
        let (payload_text, payload_bytes): (Option<String>, Option<Vec<u8>>) = match &payload {
            EventPayload::Text(s) => (Some(s.clone()), None),
            EventPayload::Bytes(b) => (None, Some(b.clone())),
            EventPayload::Empty => (None, None),
        };
        let timestamp = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;

        let result = async {
            let conn = self.db.conn().await?;
            let row = conn.query_one(
                "INSERT INTO events (name, payload_text, payload_bytes, timestamp)
                 VALUES ($1, $2, $3, $4) RETURNING id",
                &[&name, &payload_text, &payload_bytes.as_deref(), &timestamp],
            ).await?;
            let id: i64 = row.get(0);
            Ok::<i64, PgError>(id)
        }.await;

        let id = result.unwrap_or(0) as EventId;
        let bus_id = self.bus.publish_event(name.to_string(), payload);
        if id > 0 { id } else { bus_id }
    }

    /// Replay events from PostgreSQL.
    pub async fn replay(&self, from_id: EventId, filter: Option<&str>) -> Vec<Event> {
        let result = async {
            let conn = self.db.conn().await?;
            let from_id_i64 = from_id as i64;
            let rows = if let Some(prefix) = filter {
                let pattern = format!("{}%", prefix);
                conn.query(
                    "SELECT id, name, payload_text, payload_bytes, timestamp
                     FROM events WHERE id >= $1 AND name LIKE $2
                     ORDER BY id ASC",
                    &[&from_id_i64, &pattern],
                ).await?
            } else {
                conn.query(
                    "SELECT id, name, payload_text, payload_bytes, timestamp
                     FROM events WHERE id >= $1
                     ORDER BY id ASC",
                    &[&from_id_i64],
                ).await?
            };

            let mut events = Vec::new();
            for row in rows {
                let id: i64 = row.get(0);
                let name: String = row.get(1);
                let payload_text: Option<String> = row.get(2);
                let payload_bytes: Option<Vec<u8>> = row.get(3);
                let timestamp: i64 = row.get(4);

                let payload = match (payload_text, payload_bytes) {
                    (Some(s), _) => EventPayload::Text(s),
                    (None, Some(b)) => EventPayload::Bytes(b),
                    (None, None) => EventPayload::Empty,
                };

                events.push(Event { id: id as EventId, name, payload, timestamp });
            }
            Ok::<Vec<Event>, PgError>(events)
        }.await;

        result.unwrap_or_default()
    }

    /// Count events.
    pub async fn count(&self) -> Result<i64, PgError> {
        let conn = self.db.conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM events", &[]).await?;
        Ok(row.get(0))
    }
}

// ============================================================
// PgPackageStore
// ============================================================

/// PostgreSQL-backed package store.
pub struct PgPackageStore {
    db: PgDatabase,
}

impl PgPackageStore {
    /// Construct.
    pub fn new(db: PgDatabase) -> Self { Self { db } }

    /// Save a package version.
    pub async fn save(&self, pkg: &Package) -> Result<(), PgError> {
        let manifest_json = serde_json::to_string(&pkg.manifest)?;
        let trust_json = serde_json::to_string(&pkg.trust)?;
        let conn = self.db.conn().await?;
        conn.execute(
            "INSERT INTO packages (id, version, manifest_json, integrity_hash, published_at,
                install_count, active_install_count, installed, trust_json)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (id, version) DO UPDATE SET
                manifest_json = EXCLUDED.manifest_json,
                integrity_hash = EXCLUDED.integrity_hash,
                install_count = EXCLUDED.install_count,
                active_install_count = EXCLUDED.active_install_count,
                installed = EXCLUDED.installed,
                trust_json = EXCLUDED.trust_json",
            &[
                &pkg.manifest.id,
                &pkg.manifest.version.to_string(),
                &manifest_json,
                &pkg.integrity_hash,
                &pkg.published_at,
                &(pkg.install_count as i64),
                &(pkg.active_install_count as i64),
                &(pkg.installed as i32),
                &trust_json,
            ],
        ).await?;
        Ok(())
    }

    /// Mark installed.
    pub async fn mark_installed(&self, id: &str, version: &str) -> Result<(), PgError> {
        let conn = self.db.conn().await?;
        conn.execute(
            "UPDATE packages SET install_count = install_count + 1,
                active_install_count = active_install_count + 1, installed = 1
             WHERE id = $1 AND version = $2",
            &[&id, &version],
        ).await?;
        Ok(())
    }

    /// Mark uninstalled.
    pub async fn mark_uninstalled(&self, id: &str) -> Result<(), PgError> {
        let conn = self.db.conn().await?;
        conn.execute(
            "UPDATE packages SET active_install_count = GREATEST(0, active_install_count - 1),
                installed = 0 WHERE id = $1",
            &[&id],
        ).await?;
        Ok(())
    }

    /// Count packages.
    pub async fn count(&self) -> Result<i64, PgError> {
        let conn = self.db.conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM packages", &[]).await?;
        Ok(row.get(0))
    }
}

// ============================================================
// PgBillingStore (invoices + payments + subscriptions)
// ============================================================

/// PostgreSQL-backed billing store.
pub struct PgBillingStore {
    db: PgDatabase,
}

impl PgBillingStore {
    /// Construct.
    pub fn new(db: PgDatabase) -> Self { Self { db } }

    /// Save invoice.
    pub async fn save_invoice(&self, inv: &Invoice) -> Result<(), PgError> {
        let items_json = serde_json::to_string(&inv.items)?;
        let payment_ids_json = serde_json::to_string(&inv.payment_ids)?;
        let conn = self.db.conn().await?;
        conn.execute(
            "INSERT INTO invoices (id, customer_id, customer_name, items_json, total_minor,
                currency, status, created_at, due_at, paid_at, subscription_id, payment_ids_json)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status, paid_at = EXCLUDED.paid_at,
                payment_ids_json = EXCLUDED.payment_ids_json",
            &[
                &inv.id, &inv.customer_id, &inv.customer_name, &items_json,
                &(inv.total_minor as i64), &inv.currency, &inv.status.to_string(),
                &inv.created_at, &inv.due_at, &inv.paid_at, &inv.subscription_id, &payment_ids_json,
            ],
        ).await?;
        Ok(())
    }

    /// Save payment.
    pub async fn save_payment(&self, pay: &Payment) -> Result<(), PgError> {
        let conn = self.db.conn().await?;
        conn.execute(
            "INSERT INTO payments (id, invoice_id, customer_id, amount_minor, currency,
                status, method, created_at, processed_at, failure_reason)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status, processed_at = EXCLUDED.processed_at,
                failure_reason = EXCLUDED.failure_reason",
            &[
                &pay.id, &pay.invoice_id, &pay.customer_id,
                &(pay.amount_minor as i64), &pay.currency, &pay.status.to_string(),
                &pay.method, &pay.created_at, &pay.processed_at, &pay.failure_reason,
            ],
        ).await?;
        Ok(())
    }

    /// Save subscription.
    pub async fn save_subscription(&self, sub: &Subscription) -> Result<(), PgError> {
        let conn = self.db.conn().await?;
        conn.execute(
            "INSERT INTO subscriptions (id, customer_id, package_id, price_minor, currency,
                period_seconds, status, started_at, current_period_end, cancelled_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status, current_period_end = EXCLUDED.current_period_end,
                cancelled_at = EXCLUDED.cancelled_at",
            &[
                &sub.id, &sub.customer_id, &sub.package_id,
                &(sub.price_minor as i64), &sub.currency,
                &(sub.period_seconds as i64), &sub.status.to_string(),
                &sub.started_at, &sub.current_period_end, &sub.cancelled_at,
            ],
        ).await?;
        Ok(())
    }

    /// Count invoices.
    pub async fn invoice_count(&self) -> Result<i64, PgError> {
        let conn = self.db.conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM invoices", &[]).await?;
        Ok(row.get(0))
    }

    /// Count payments.
    pub async fn payment_count(&self) -> Result<i64, PgError> {
        let conn = self.db.conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM payments", &[]).await?;
        Ok(row.get(0))
    }

    /// Count subscriptions.
    pub async fn subscription_count(&self) -> Result<i64, PgError> {
        let conn = self.db.conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM subscriptions", &[]).await?;
        Ok(row.get(0))
    }
}

// ============================================================
// PgWorkflowStore
// ============================================================

/// PostgreSQL-backed workflow store.
pub struct PgWorkflowStore {
    db: PgDatabase,
}

impl PgWorkflowStore {
    /// Construct.
    pub fn new(db: PgDatabase) -> Self { Self { db } }

    /// Save workflow.
    pub async fn save_workflow(&self, wf: &Workflow) -> Result<(), PgError> {
        let trigger_json = serde_json::to_string(&wf.trigger)?;
        let steps_json = serde_json::to_string(&wf.steps)?;
        let conn = self.db.conn().await?;
        conn.execute(
            "INSERT INTO workflows (id, name, description, trigger_json, steps_json,
                enabled, created_at, execution_count)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name, enabled = EXCLUDED.enabled,
                execution_count = EXCLUDED.execution_count",
            &[
                &wf.id, &wf.name, &wf.description, &trigger_json, &steps_json,
                &(wf.enabled as i32), &wf.created_at, &(wf.execution_count as i64),
            ],
        ).await?;
        Ok(())
    }

    /// Save execution.
    pub async fn save_execution(&self, exec: &WorkflowExecution) -> Result<(), PgError> {
        let step_results_json = serde_json::to_string(&exec.step_results)?;
        let conn = self.db.conn().await?;
        conn.execute(
            "INSERT INTO workflow_executions (id, workflow_id, trigger_event, trigger_payload,
                status, step_results_json, started_at, finished_at, error)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (id) DO NOTHING",
            &[
                &exec.id, &exec.workflow_id, &exec.trigger_event, &exec.trigger_payload,
                &exec.status.to_string(), &step_results_json,
                &exec.started_at, &exec.finished_at, &exec.error,
            ],
        ).await?;
        Ok(())
    }

    /// Count workflows.
    pub async fn workflow_count(&self) -> Result<i64, PgError> {
        let conn = self.db.conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM workflows", &[]).await?;
        Ok(row.get(0))
    }

    /// Count executions.
    pub async fn execution_count(&self) -> Result<i64, PgError> {
        let conn = self.db.conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM workflow_executions", &[]).await?;
        Ok(row.get(0))
    }
}

// ============================================================
// PgNotificationStore
// ============================================================

/// PostgreSQL-backed notification store.
pub struct PgNotificationStore {
    db: PgDatabase,
}

impl PgNotificationStore {
    /// Construct.
    pub fn new(db: PgDatabase) -> Self { Self { db } }

    /// Save notification.
    pub async fn save(&self, n: &Notification) -> Result<(), PgError> {
        let conn = self.db.conn().await?;
        conn.execute(
            "INSERT INTO notifications (id, user_id, title, body, severity, read, created_at, link, icon)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (id) DO UPDATE SET read = EXCLUDED.read",
            &[
                &n.id, &n.user_id, &n.title, &n.body, &n.severity.to_string(),
                &(n.read as i32), &n.created_at, &n.link, &n.icon,
            ],
        ).await?;
        Ok(())
    }

    /// Mark read.
    pub async fn mark_read(&self, id: &str) -> Result<(), PgError> {
        let conn = self.db.conn().await?;
        conn.execute("UPDATE notifications SET read = 1 WHERE id = $1", &[&id]).await?;
        Ok(())
    }

    /// Count.
    pub async fn count(&self) -> Result<i64, PgError> {
        let conn = self.db.conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM notifications", &[]).await?;
        Ok(row.get(0))
    }

    /// Unread count for user.
    pub async fn unread_count(&self, user_id: &str) -> Result<i64, PgError> {
        let conn = self.db.conn().await?;
        let row = conn.query_one(
            "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read = 0",
            &[&user_id],
        ).await?;
        Ok(row.get(0))
    }
}

// ============================================================
// PgTenancyStore
// ============================================================

/// PostgreSQL-backed tenancy store (organizations, memberships, teams).
pub struct PgTenancyStore {
    db: PgDatabase,
}

impl PgTenancyStore {
    /// Construct.
    pub fn new(db: PgDatabase) -> Self { Self { db } }

    /// Save organization (UPSERT).
    pub async fn save_org(&self, org: &nexora_tenancy::types::Organization) -> Result<(), PgError> {
        let conn = self.db.conn().await?;
        conn.execute(
            "INSERT INTO organizations (id, name, slug, tier, owner_id, description, active, created_at, max_members)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name, slug = EXCLUDED.slug, tier = EXCLUDED.tier,
                active = EXCLUDED.active, max_members = EXCLUDED.max_members",
            &[
                &org.id, &org.name, &org.slug, &org.tier.to_string(),
                &org.owner_id, &org.description,
                &(org.active as i32), &org.created_at,
                &(org.max_members as i32),
            ],
        ).await?;
        Ok(())
    }

    /// Save membership (UPSERT on composite PK).
    pub async fn save_membership(&self, m: &nexora_tenancy::types::Membership) -> Result<(), PgError> {
        let conn = self.db.conn().await?;
        conn.execute(
            "INSERT INTO org_memberships (org_id, user_id, role, joined_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (org_id, user_id) DO UPDATE SET role = EXCLUDED.role",
            &[&m.org_id, &m.user_id, &m.role.to_string(), &m.joined_at],
        ).await?;
        Ok(())
    }

    /// Delete membership.
    pub async fn delete_membership(&self, org_id: &str, user_id: &str) -> Result<(), PgError> {
        let conn = self.db.conn().await?;
        conn.execute(
            "DELETE FROM org_memberships WHERE org_id = $1 AND user_id = $2",
            &[&org_id, &user_id],
        ).await?;
        Ok(())
    }

    /// Save team (UPSERT).
    pub async fn save_team(&self, team: &nexora_tenancy::types::Team) -> Result<(), PgError> {
        let member_ids_json = serde_json::to_string(&team.member_ids)?;
        let conn = self.db.conn().await?;
        conn.execute(
            "INSERT INTO teams (id, org_id, name, description, member_ids_json, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name, description = EXCLUDED.description,
                member_ids_json = EXCLUDED.member_ids_json",
            &[
                &team.id, &team.org_id, &team.name, &team.description,
                &member_ids_json, &team.created_at,
            ],
        ).await?;
        Ok(())
    }

    /// Delete team.
    pub async fn delete_team(&self, id: &str) -> Result<(), PgError> {
        let conn = self.db.conn().await?;
        conn.execute("DELETE FROM teams WHERE id = $1", &[&id]).await?;
        Ok(())
    }

    /// Count organizations.
    pub async fn org_count(&self) -> Result<i64, PgError> {
        let conn = self.db.conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM organizations", &[]).await?;
        Ok(row.get(0))
    }

    /// Count memberships.
    pub async fn membership_count(&self) -> Result<i64, PgError> {
        let conn = self.db.conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM org_memberships", &[]).await?;
        Ok(row.get(0))
    }

    /// Count teams.
    pub async fn team_count(&self) -> Result<i64, PgError> {
        let conn = self.db.conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM teams", &[]).await?;
        Ok(row.get(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_user_store_exists() {
        // Type check only — actual PostgreSQL tests require a running DB.
        let _ = std::any::TypeId::of::<PgUserStore>();
    }

    #[test]
    fn pg_event_store_exists() {
        let _ = std::any::TypeId::of::<PgEventStore>();
    }

    #[test]
    fn pg_package_store_exists() {
        let _ = std::any::TypeId::of::<PgPackageStore>();
    }

    #[test]
    fn pg_billing_store_exists() {
        let _ = std::any::TypeId::of::<PgBillingStore>();
    }

    #[test]
    fn pg_workflow_store_exists() {
        let _ = std::any::TypeId::of::<PgWorkflowStore>();
    }

    #[test]
    fn pg_notification_store_exists() {
        let _ = std::any::TypeId::of::<PgNotificationStore>();
    }

    #[test]
    fn pg_tenancy_store_exists() {
        let _ = std::any::TypeId::of::<PgTenancyStore>();
    }
}
