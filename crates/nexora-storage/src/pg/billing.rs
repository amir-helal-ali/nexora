//! PostgreSQL billing store — invoices, payments, subscriptions.

use crate::pg::{PgError, PgPool};
use nexora_billing::types::{
    Invoice, InvoiceId, InvoiceItem, InvoiceStatus,
    Payment, PaymentId, PaymentStatus,
    Subscription, SubscriptionId, SubscriptionStatus,
};

/// PostgreSQL billing store.
pub struct PgBillingStore {
    pool: PgPool,
}

impl PgBillingStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Upsert an invoice.
    pub async fn upsert_invoice(&self, inv: &Invoice) -> Result<(), PgError> {
        let items_json = serde_json::to_value(&inv.items)?;
        let payment_ids_json = serde_json::to_value(&inv.payment_ids)?;
        let status_str = inv.status.to_string();
        self.pool.execute(
            r#"INSERT INTO invoices (
                id, customer_id, customer_name, items, total_minor,
                currency, status, created_at, due_at, paid_at,
                subscription_id, payment_ids
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12
            )
            ON CONFLICT (id) DO UPDATE SET
                customer_name = EXCLUDED.customer_name,
                items = EXCLUDED.items,
                total_minor = EXCLUDED.total_minor,
                currency = EXCLUDED.currency,
                status = EXCLUDED.status,
                paid_at = EXCLUDED.paid_at,
                payment_ids = EXCLUDED.payment_ids,
                subscription_id = EXCLUDED.subscription_id"#,
            &[
                &inv.id as &(dyn postgres_types::ToSql + Sync),
                &inv.customer_id,
                &inv.customer_name,
                &items_json,
                &(inv.total_minor as i64),
                &inv.currency,
                &status_str,
                &inv.created_at,
                &inv.due_at,
                &inv.paid_at,
                &inv.subscription_id,
                &payment_ids_json,
            ],
        ).await?;
        Ok(())
    }

    /// Update invoice status only.
    pub async fn update_invoice_status(
        &self,
        id: &str,
        status: &InvoiceStatus,
        paid_at: Option<i64>,
    ) -> Result<u64, PgError> {
        let status_str = status.to_string();
        self.pool.execute(
            "UPDATE invoices SET status = $1, paid_at = COALESCE($2, paid_at) WHERE id = $3",
            &[&status_str, &paid_at, &id],
        ).await
    }

    /// Fetch an invoice by ID.
    pub async fn get_invoice(&self, id: &str) -> Result<Option<Invoice>, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_opt(
            "SELECT id, customer_id, customer_name, items, total_minor,
                    currency, status, created_at, due_at, paid_at,
                    subscription_id, payment_ids
             FROM invoices WHERE id = $1",
            &[&id],
        ).await?;
        match row {
            Some(r) => Ok(Some(Self::row_to_invoice(&r)?)),
            None => Ok(None),
        }
    }

    /// List invoices for a customer.
    pub async fn list_invoices_for_customer(
        &self,
        customer_id: &str,
        limit: i64,
    ) -> Result<Vec<Invoice>, PgError> {
        let conn = self.pool.get_conn().await?;
        let rows = conn.query(
            "SELECT id, customer_id, customer_name, items, total_minor,
                    currency, status, created_at, due_at, paid_at,
                    subscription_id, payment_ids
             FROM invoices WHERE customer_id = $1
             ORDER BY created_at DESC LIMIT $2",
            &[&customer_id, &limit],
        ).await?;
        rows.iter().map(Self::row_to_invoice).collect()
    }

    /// Count invoices.
    pub async fn invoice_count(&self) -> Result<i64, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM invoices", &[]).await?;
        Ok(row.get(0))
    }

    /// Upsert a payment.
    pub async fn upsert_payment(&self, p: &Payment) -> Result<(), PgError> {
        let status_str = p.status.to_string();
        self.pool.execute(
            r#"INSERT INTO payments (
                id, invoice_id, customer_id, amount_minor, currency,
                status, provider, provider_txn_id, created_at, completed_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10
            )
            ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status,
                completed_at = EXCLUDED.completed_at,
                provider_txn_id = EXCLUDED.provider_txn_id"#,
            &[
                &p.id as &(dyn postgres_types::ToSql + Sync),
                &Some(p.invoice_id.clone()),
                &p.customer_id,
                &(p.amount_minor as i64),
                &p.currency,
                &status_str,
                &Some(p.method.clone()),
                &p.failure_reason,
                &p.created_at,
                &p.processed_at,
            ],
        ).await?;
        Ok(())
    }

    /// Count payments.
    pub async fn payment_count(&self) -> Result<i64, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM payments", &[]).await?;
        Ok(row.get(0))
    }

    /// Upsert a subscription.
    pub async fn upsert_subscription(&self, s: &Subscription) -> Result<(), PgError> {
        let status_str = s.status.to_string();
        self.pool.execute(
            r#"INSERT INTO subscriptions (
                id, customer_id, package_id, status, period_start,
                period_end, amount_minor, currency, created_at, cancelled_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10
            )
            ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status,
                period_end = EXCLUDED.period_end,
                cancelled_at = EXCLUDED.cancelled_at"#,
            &[
                &s.id as &(dyn postgres_types::ToSql + Sync),
                &s.customer_id,
                &s.package_id,
                &status_str,
                &s.started_at,
                &s.current_period_end,
                &(s.price_minor as i64),
                &s.currency,
                &s.started_at,
                &s.cancelled_at,
            ],
        ).await?;
        Ok(())
    }

    /// Count subscriptions.
    pub async fn subscription_count(&self) -> Result<i64, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM subscriptions", &[]).await?;
        Ok(row.get(0))
    }

    fn row_to_invoice(row: &tokio_postgres::Row) -> Result<Invoice, PgError> {
        let items_json: serde_json::Value = row.get(3);
        let items: Vec<InvoiceItem> = serde_json::from_value(items_json)?;
        let payment_ids_json: serde_json::Value = row.get(11);
        let payment_ids: Vec<PaymentId> = serde_json::from_value(payment_ids_json)?;
        let status_str: String = row.get(6);
        let status = parse_invoice_status(&status_str);
        let total_minor: i64 = row.get(4);
        Ok(Invoice {
            id: row.get(0),
            customer_id: row.get(1),
            customer_name: row.get(2),
            items,
            total_minor: total_minor as u64,
            currency: row.get(5),
            status,
            created_at: row.get(7),
            due_at: row.get(8),
            paid_at: row.get(9),
            subscription_id: row.get(10),
            payment_ids,
        })
    }
}

fn parse_invoice_status(s: &str) -> InvoiceStatus {
    match s {
        "draft" => InvoiceStatus::Draft,
        "open" => InvoiceStatus::Open,
        "paid" => InvoiceStatus::Paid,
        "past_due" => InvoiceStatus::PastDue,
        "void" => InvoiceStatus::Void,
        "refunded" => InvoiceStatus::Refunded,
        _ => InvoiceStatus::Draft,
    }
}
