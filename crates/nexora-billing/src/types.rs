//! Billing types — invoices, payments, subscriptions.
//!
//! These types model the financial state of the platform. Every monetary
//! amount is stored in **minor units** (e.g. cents) to avoid floating-point
//! rounding errors. Currency is always ISO 4217.

use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;

/// Unique invoice ID (UUID v4).
pub type InvoiceId = String;
/// Unique payment ID (UUID v4).
pub type PaymentId = String;
/// Unique subscription ID (UUID v4).
pub type SubscriptionId = String;

/// Invoice status lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    /// Invoice created but not yet sent to the customer.
    Draft,
    /// Invoice sent, awaiting payment.
    Open,
    /// Payment received in full.
    Paid,
    /// Payment past due.
    PastDue,
    /// Invoice voided (cancelled before payment).
    Void,
    /// Invoice fully refunded.
    Refunded,
}

impl fmt::Display for InvoiceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Draft => f.write_str("draft"),
            Self::Open => f.write_str("open"),
            Self::Paid => f.write_str("paid"),
            Self::PastDue => f.write_str("past_due"),
            Self::Void => f.write_str("void"),
            Self::Refunded => f.write_str("refunded"),
        }
    }
}

/// Payment status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    /// Payment initiated, awaiting confirmation.
    Pending,
    /// Payment confirmed (funds received).
    Succeeded,
    /// Payment failed.
    Failed,
    /// Payment refunded.
    Refunded,
}

impl fmt::Display for PaymentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => f.write_str("pending"),
            Self::Succeeded => f.write_str("succeeded"),
            Self::Failed => f.write_str("failed"),
            Self::Refunded => f.write_str("refunded"),
        }
    }
}

/// Subscription status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    /// Subscription is active and billing.
    Active,
    /// Subscription is paused (not billing, but not cancelled).
    Paused,
    /// Subscription past due (payment failed but grace period active).
    PastDue,
    /// Subscription cancelled (no more billing).
    Cancelled,
    /// Subscription ended (cancelled + period expired).
    Ended,
}

impl fmt::Display for SubscriptionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => f.write_str("active"),
            Self::Paused => f.write_str("paused"),
            Self::PastDue => f.write_str("past_due"),
            Self::Cancelled => f.write_str("cancelled"),
            Self::Ended => f.write_str("ended"),
        }
    }
}

/// A single line item on an invoice.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InvoiceItem {
    /// Description of the item.
    pub description: String,
    /// Package ID this item refers to (if applicable).
    pub package_id: Option<String>,
    /// Quantity (for usage-based billing).
    pub quantity: u64,
    /// Unit price in minor units.
    pub unit_price_minor: u64,
    /// Total price = quantity × unit_price_minor.
    pub total_minor: u64,
    /// ISO 4217 currency code.
    pub currency: String,
}

/// An invoice.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Invoice {
    /// Unique invoice ID.
    pub id: InvoiceId,
    /// Customer user ID.
    pub customer_id: String,
    /// Customer display name (snapshot at creation time).
    pub customer_name: String,
    /// Line items.
    pub items: Vec<InvoiceItem>,
    /// Total amount in minor units.
    pub total_minor: u64,
    /// ISO 4217 currency code.
    pub currency: String,
    /// Current status.
    pub status: InvoiceStatus,
    /// When the invoice was created (unix nanos).
    pub created_at: i64,
    /// When the invoice is due (unix nanos).
    pub due_at: i64,
    /// When the invoice was paid (unix nanos), if applicable.
    pub paid_at: Option<i64>,
    /// Associated subscription ID (for recurring invoices).
    pub subscription_id: Option<SubscriptionId>,
    /// Associated payment IDs.
    pub payment_ids: Vec<PaymentId>,
}

impl Invoice {
    /// Construct a new draft invoice.
    pub fn new_draft(customer_id: &str, customer_name: &str, currency: &str) -> Self {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let due = now + 30 * 86_400 * 1_000_000_000; // 30 days
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            customer_id: customer_id.to_string(),
            customer_name: customer_name.to_string(),
            items: vec![],
            total_minor: 0,
            currency: currency.to_string(),
            status: InvoiceStatus::Draft,
            created_at: now,
            due_at: due,
            paid_at: None,
            subscription_id: None,
            payment_ids: vec![],
        }
    }

    /// Add a line item.
    pub fn add_item(&mut self, item: InvoiceItem) {
        self.total_minor += item.total_minor;
        self.items.push(item);
    }

    /// Mark the invoice as open (sent to customer).
    pub fn mark_open(&mut self) {
        self.status = InvoiceStatus::Open;
    }

    /// Mark the invoice as paid.
    pub fn mark_paid(&mut self) {
        self.status = InvoiceStatus::Paid;
        self.paid_at = Some(OffsetDateTime::now_utc().unix_timestamp_nanos() as i64);
    }

    /// Void the invoice.
    pub fn void(&mut self) {
        self.status = InvoiceStatus::Void;
    }
}

/// A payment against an invoice.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Payment {
    /// Unique payment ID.
    pub id: PaymentId,
    /// Invoice this payment is for.
    pub invoice_id: InvoiceId,
    /// Customer user ID.
    pub customer_id: String,
    /// Amount in minor units.
    pub amount_minor: u64,
    /// ISO 4217 currency code.
    pub currency: String,
    /// Payment status.
    pub status: PaymentStatus,
    /// Payment method (e.g. "card", "bank_transfer", "credit").
    pub method: String,
    /// When the payment was created (unix nanos).
    pub created_at: i64,
    /// When the payment was processed (unix nanos).
    pub processed_at: Option<i64>,
    /// Optional failure reason.
    pub failure_reason: Option<String>,
}

impl Payment {
    /// Construct a new pending payment.
    pub fn new_pending(
        invoice_id: &str,
        customer_id: &str,
        amount_minor: u64,
        currency: &str,
        method: &str,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            invoice_id: invoice_id.to_string(),
            customer_id: customer_id.to_string(),
            amount_minor,
            currency: currency.to_string(),
            status: PaymentStatus::Pending,
            method: method.to_string(),
            created_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            processed_at: None,
            failure_reason: None,
        }
    }

    /// Mark the payment as succeeded.
    pub fn succeed(&mut self) {
        self.status = PaymentStatus::Succeeded;
        self.processed_at = Some(OffsetDateTime::now_utc().unix_timestamp_nanos() as i64);
    }

    /// Mark the payment as failed.
    pub fn fail(&mut self, reason: &str) {
        self.status = PaymentStatus::Failed;
        self.processed_at = Some(OffsetDateTime::now_utc().unix_timestamp_nanos() as i64);
        self.failure_reason = Some(reason.to_string());
    }
}

/// A subscription (recurring billing).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Subscription {
    /// Unique subscription ID.
    pub id: SubscriptionId,
    /// Customer user ID.
    pub customer_id: String,
    /// Package ID being subscribed to.
    pub package_id: String,
    /// Price per period in minor units.
    pub price_minor: u64,
    /// ISO 4217 currency code.
    pub currency: String,
    /// Billing period in seconds (e.g. 2592000 for monthly).
    pub period_seconds: u64,
    /// Current status.
    pub status: SubscriptionStatus,
    /// When the subscription started (unix nanos).
    pub started_at: i64,
    /// When the current period ends (unix nanos).
    pub current_period_end: i64,
    /// When the subscription was cancelled (unix nanos), if applicable.
    pub cancelled_at: Option<i64>,
}

impl Subscription {
    /// Construct a new active subscription.
    pub fn new_active(
        customer_id: &str,
        package_id: &str,
        price_minor: u64,
        currency: &str,
        period_seconds: u64,
    ) -> Self {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            customer_id: customer_id.to_string(),
            package_id: package_id.to_string(),
            price_minor,
            currency: currency.to_string(),
            period_seconds,
            status: SubscriptionStatus::Active,
            started_at: now,
            current_period_end: now + period_seconds as i64 * 1_000_000_000,
            cancelled_at: None,
        }
    }

    /// Check if the current period has ended (needs renewal).
    pub fn needs_renewal(&self) -> bool {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        now >= self.current_period_end && self.status == SubscriptionStatus::Active
    }

    /// Renew the subscription for the next period. If the current period has
    /// already ended, the new period starts from now; otherwise it extends
    /// from the current end.
    pub fn renew(&mut self) {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let period_ns = self.period_seconds as i64 * 1_000_000_000;
        // If the current period already ended, start from now.
        if self.current_period_end < now {
            self.current_period_end = now + period_ns;
        } else {
            self.current_period_end += period_ns;
        }
    }

    /// Cancel the subscription (takes effect at period end).
    pub fn cancel(&mut self) {
        self.status = SubscriptionStatus::Cancelled;
        self.cancelled_at = Some(OffsetDateTime::now_utc().unix_timestamp_nanos() as i64);
    }

    /// Generate an invoice for the current period.
    pub fn generate_invoice(&self, customer_name: &str) -> Invoice {
        let mut invoice = Invoice::new_draft(&self.customer_id, customer_name, &self.currency);
        invoice.subscription_id = Some(self.id.clone());
        invoice.add_item(InvoiceItem {
            description: format!("Subscription: {} (1 period)", self.package_id),
            package_id: Some(self.package_id.clone()),
            quantity: 1,
            unit_price_minor: self.price_minor,
            total_minor: self.price_minor,
            currency: self.currency.clone(),
        });
        invoice.mark_open();
        invoice
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoice_lifecycle() {
        let mut inv = Invoice::new_draft("u1", "Alice", "USD");
        assert_eq!(inv.status, InvoiceStatus::Draft);
        assert_eq!(inv.total_minor, 0);

        inv.add_item(InvoiceItem {
            description: "Item 1".into(),
            package_id: Some("com.test.pkg".into()),
            quantity: 2,
            unit_price_minor: 500,
            total_minor: 1000,
            currency: "USD".into(),
        });
        assert_eq!(inv.total_minor, 1000);

        inv.mark_open();
        assert_eq!(inv.status, InvoiceStatus::Open);

        inv.mark_paid();
        assert_eq!(inv.status, InvoiceStatus::Paid);
        assert!(inv.paid_at.is_some());
    }

    #[test]
    fn payment_lifecycle() {
        let mut pay = Payment::new_pending("inv1", "u1", 1000, "USD", "card");
        assert_eq!(pay.status, PaymentStatus::Pending);
        assert!(pay.processed_at.is_none());

        pay.succeed();
        assert_eq!(pay.status, PaymentStatus::Succeeded);
        assert!(pay.processed_at.is_some());

        let mut pay2 = Payment::new_pending("inv1", "u1", 1000, "USD", "card");
        pay2.fail("insufficient funds");
        assert_eq!(pay2.status, PaymentStatus::Failed);
        assert_eq!(pay2.failure_reason, Some("insufficient funds".into()));
    }

    #[test]
    fn subscription_lifecycle() {
        let mut sub = Subscription::new_active("u1", "com.test.pkg", 999, "USD", 2592000);
        assert_eq!(sub.status, SubscriptionStatus::Active);
        assert!(!sub.needs_renewal()); // just started

        // Simulate period end by backdating.
        sub.current_period_end = 0;
        assert!(sub.needs_renewal());

        sub.renew();
        assert!(!sub.needs_renewal());

        sub.cancel();
        assert_eq!(sub.status, SubscriptionStatus::Cancelled);
        assert!(sub.cancelled_at.is_some());
    }

    #[test]
    fn subscription_generates_invoice() {
        let sub = Subscription::new_active("u1", "com.test.pkg", 999, "USD", 2592000);
        let inv = sub.generate_invoice("Alice");
        assert_eq!(inv.customer_id, "u1");
        assert_eq!(inv.total_minor, 999);
        assert_eq!(inv.status, InvoiceStatus::Open);
        assert_eq!(inv.subscription_id, Some(sub.id));
        assert_eq!(inv.items.len(), 1);
    }
}
