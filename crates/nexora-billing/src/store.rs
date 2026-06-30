//! Billing store — in-memory registry of invoices, payments, subscriptions.
//!
//! Thread-safe. Emits events on every state change:
//! - invoice.created, invoice.paid, invoice.voided
//! - payment.created, payment.succeeded, payment.failed
//! - subscription.created, subscription.renewed, subscription.cancelled

use crate::types::{
    Invoice, InvoiceId, InvoiceStatus, Payment, PaymentId, PaymentStatus, Subscription,
    SubscriptionId, SubscriptionStatus,
};
use nexora_core::events::EventPayload;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use time::OffsetDateTime;

/// Error from billing store operations.
#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    /// Invoice not found.
    #[error("invoice not found: {0}")]
    InvoiceNotFound(InvoiceId),
    /// Payment not found.
    #[error("payment not found: {0}")]
    PaymentNotFound(PaymentId),
    /// Subscription not found.
    #[error("subscription not found: {0}")]
    SubscriptionNotFound(SubscriptionId),
    /// Invoice is in the wrong state for this operation.
    #[error("invoice {id} in state {state}, expected {expected}")]
    InvoiceWrongState {
        /// Invoice ID.
        id: InvoiceId,
        /// Current state.
        state: InvoiceStatus,
        /// Expected state.
        expected: InvoiceStatus,
    },
    /// Payment is in the wrong state.
    #[error("payment {id} in state {state}, expected {expected}")]
    PaymentWrongState {
        /// Payment ID.
        id: PaymentId,
        /// Current state.
        state: PaymentStatus,
        /// Expected state.
        expected: PaymentStatus,
    },
    /// Subscription is in the wrong state.
    #[error("subscription {id} in state {state}, expected {expected}")]
    SubscriptionWrongState {
        /// Subscription ID.
        id: SubscriptionId,
        /// Current state.
        state: SubscriptionStatus,
        /// Expected state.
        expected: SubscriptionStatus,
    },
    /// Payment amount doesn't match invoice.
    #[error("payment amount ({paid}) doesn't match invoice total ({expected})")]
    AmountMismatch {
        /// Paid amount.
        paid: u64,
        /// Expected amount.
        expected: u64,
    },
}

/// The billing store. Thread-safe.
pub struct BillingStore {
    invoices: RwLock<HashMap<InvoiceId, Invoice>>,
    payments: RwLock<HashMap<PaymentId, Payment>>,
    subscriptions: RwLock<HashMap<SubscriptionId, Subscription>>,
    /// Map: customer ID → list of invoice IDs.
    invoices_by_customer: RwLock<HashMap<String, Vec<InvoiceId>>>,
    /// Map: customer ID → list of subscription IDs.
    subs_by_customer: RwLock<HashMap<String, Vec<SubscriptionId>>>,
    event_bus: Option<Arc<nexora_core::EventBus>>,
}

impl fmt::Debug for BillingStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inv = self.invoices.read().len();
        let pay = self.payments.read().len();
        let sub = self.subscriptions.read().len();
        f.debug_struct("BillingStore")
            .field("invoices", &inv)
            .field("payments", &pay)
            .field("subscriptions", &sub)
            .finish()
    }
}

impl Default for BillingStore {
    fn default() -> Self {
        Self::new()
    }
}

impl BillingStore {
    /// Construct an empty billing store.
    pub fn new() -> Self {
        Self {
            invoices: RwLock::new(HashMap::new()),
            payments: RwLock::new(HashMap::new()),
            subscriptions: RwLock::new(HashMap::new()),
            invoices_by_customer: RwLock::new(HashMap::new()),
            subs_by_customer: RwLock::new(HashMap::new()),
            event_bus: None,
        }
    }

    /// Attach an EventBus for event publishing.
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Number of invoices.
    pub fn invoice_count(&self) -> usize {
        self.invoices.read().len()
    }

    /// Number of payments.
    pub fn payment_count(&self) -> usize {
        self.payments.read().len()
    }

    /// Number of subscriptions.
    pub fn subscription_count(&self) -> usize {
        self.subscriptions.read().len()
    }

    // ---- Invoices ----

    /// Create a new invoice.
    pub fn create_invoice(&self, mut invoice: Invoice) -> Result<Invoice, BillingError> {
        let id = invoice.id.clone();
        let customer = invoice.customer_id.clone();
        invoice.created_at = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        self.invoices.write().insert(id.clone(), invoice.clone());
        self.invoices_by_customer
            .write()
            .entry(customer)
            .or_default()
            .push(id.clone());
        self.emit("invoice.created", &id);
        Ok(invoice)
    }

    /// Get an invoice by ID.
    pub fn get_invoice(&self, id: &str) -> Option<Invoice> {
        self.invoices.read().get(id).cloned()
    }

    /// List invoices for a customer.
    pub fn list_invoices_for_customer(&self, customer_id: &str) -> Vec<Invoice> {
        let by_cust = self.invoices_by_customer.read();
        let invoices = self.invoices.read();
        by_cust
            .get(customer_id)
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|id| invoices.get(id).cloned())
            .collect()
    }

    /// List all invoices.
    pub fn list_invoices(&self) -> Vec<Invoice> {
        self.invoices.read().values().cloned().collect()
    }

    /// Mark an invoice as paid (called after a successful payment).
    pub fn mark_invoice_paid(&self, id: &str) -> Result<Invoice, BillingError> {
        let mut invoices = self.invoices.write();
        let invoice = invoices
            .get_mut(id)
            .ok_or_else(|| BillingError::InvoiceNotFound(id.to_string()))?;
        if invoice.status != InvoiceStatus::Open && invoice.status != InvoiceStatus::PastDue {
            return Err(BillingError::InvoiceWrongState {
                id: id.to_string(),
                state: invoice.status,
                expected: InvoiceStatus::Open,
            });
        }
        invoice.mark_paid();
        let invoice = invoice.clone();
        drop(invoices);
        self.emit("invoice.paid", id);
        Ok(invoice)
    }

    /// Void an invoice.
    pub fn void_invoice(&self, id: &str) -> Result<Invoice, BillingError> {
        let mut invoices = self.invoices.write();
        let invoice = invoices
            .get_mut(id)
            .ok_or_else(|| BillingError::InvoiceNotFound(id.to_string()))?;
        if invoice.status == InvoiceStatus::Paid || invoice.status == InvoiceStatus::Refunded {
            return Err(BillingError::InvoiceWrongState {
                id: id.to_string(),
                state: invoice.status,
                expected: InvoiceStatus::Open,
            });
        }
        invoice.void();
        let invoice = invoice.clone();
        drop(invoices);
        self.emit("invoice.voided", id);
        Ok(invoice)
    }

    // ---- Payments ----

    /// Record a payment and link it to its invoice.
    pub fn create_payment(&self, payment: Payment) -> Result<Payment, BillingError> {
        let id = payment.id.clone();
        let invoice_id = payment.invoice_id.clone();
        self.payments.write().insert(id.clone(), payment.clone());

        // Link payment to invoice.
        let mut invoices = self.invoices.write();
        if let Some(inv) = invoices.get_mut(&invoice_id) {
            inv.payment_ids.push(id.clone());
        }
        drop(invoices);

        self.emit("payment.created", &id);
        Ok(payment)
    }

    /// Mark a payment as succeeded. Also marks the associated invoice as paid
    /// if the payment amount matches.
    pub fn succeed_payment(&self, id: &str) -> Result<(Payment, Option<Invoice>), BillingError> {
        let mut payments = self.payments.write();
        let payment = payments
            .get_mut(id)
            .ok_or_else(|| BillingError::PaymentNotFound(id.to_string()))?;
        if payment.status != PaymentStatus::Pending {
            return Err(BillingError::PaymentWrongState {
                id: id.to_string(),
                state: payment.status,
                expected: PaymentStatus::Pending,
            });
        }
        payment.succeed();
        let payment = payment.clone();
        let invoice_id = payment.invoice_id.clone();
        let amount = payment.amount_minor;
        drop(payments);

        self.emit("payment.succeeded", id);

        // Mark the invoice as paid if amount matches.
        let mut invoices = self.invoices.write();
        if let Some(inv) = invoices.get_mut(&invoice_id) {
            if inv.total_minor != amount {
                let expected = inv.total_minor;
                drop(invoices);
                return Err(BillingError::AmountMismatch {
                    paid: amount,
                    expected,
                });
            }
            inv.mark_paid();
            let inv_clone = inv.clone();
            drop(invoices);
            self.emit("invoice.paid", &invoice_id);
            Ok((payment, Some(inv_clone)))
        } else {
            drop(invoices);
            Ok((payment, None))
        }
    }

    /// Mark a payment as failed.
    pub fn fail_payment(&self, id: &str, reason: &str) -> Result<Payment, BillingError> {
        let mut payments = self.payments.write();
        let payment = payments
            .get_mut(id)
            .ok_or_else(|| BillingError::PaymentNotFound(id.to_string()))?;
        if payment.status != PaymentStatus::Pending {
            return Err(BillingError::PaymentWrongState {
                id: id.to_string(),
                state: payment.status,
                expected: PaymentStatus::Pending,
            });
        }
        payment.fail(reason);
        let payment = payment.clone();
        drop(payments);
        self.emit("payment.failed", id);
        Ok(payment)
    }

    /// Get a payment by ID.
    pub fn get_payment(&self, id: &str) -> Option<Payment> {
        self.payments.read().get(id).cloned()
    }

    /// List all payments.
    pub fn list_payments(&self) -> Vec<Payment> {
        self.payments.read().values().cloned().collect()
    }

    // ---- Subscriptions ----

    /// Create a new subscription.
    pub fn create_subscription(&self, sub: Subscription) -> Result<Subscription, BillingError> {
        let id = sub.id.clone();
        let customer = sub.customer_id.clone();
        self.subscriptions.write().insert(id.clone(), sub.clone());
        self.subs_by_customer
            .write()
            .entry(customer)
            .or_default()
            .push(id.clone());
        self.emit("subscription.created", &id);
        Ok(sub)
    }

    /// Get a subscription by ID.
    pub fn get_subscription(&self, id: &str) -> Option<Subscription> {
        self.subscriptions.read().get(id).cloned()
    }

    /// List subscriptions for a customer.
    pub fn list_subscriptions_for_customer(&self, customer_id: &str) -> Vec<Subscription> {
        let by_cust = self.subs_by_customer.read();
        let subs = self.subscriptions.read();
        by_cust
            .get(customer_id)
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|id| subs.get(id).cloned())
            .collect()
    }

    /// List all subscriptions.
    pub fn list_subscriptions(&self) -> Vec<Subscription> {
        self.subscriptions.read().values().cloned().collect()
    }

    /// Cancel a subscription.
    pub fn cancel_subscription(&self, id: &str) -> Result<Subscription, BillingError> {
        let mut subs = self.subscriptions.write();
        let sub = subs
            .get_mut(id)
            .ok_or_else(|| BillingError::SubscriptionNotFound(id.to_string()))?;
        if sub.status != SubscriptionStatus::Active && sub.status != SubscriptionStatus::PastDue {
            return Err(BillingError::SubscriptionWrongState {
                id: id.to_string(),
                state: sub.status,
                expected: SubscriptionStatus::Active,
            });
        }
        sub.cancel();
        let sub = sub.clone();
        drop(subs);
        self.emit("subscription.cancelled", id);
        Ok(sub)
    }

    /// Renew a subscription (generates a new invoice).
    pub fn renew_subscription(&self, id: &str, customer_name: &str) -> Result<(Subscription, Invoice), BillingError> {
        let mut subs = self.subscriptions.write();
        let sub = subs
            .get_mut(id)
            .ok_or_else(|| BillingError::SubscriptionNotFound(id.to_string()))?;
        if sub.status != SubscriptionStatus::Active {
            return Err(BillingError::SubscriptionWrongState {
                id: id.to_string(),
                state: sub.status,
                expected: SubscriptionStatus::Active,
            });
        }
        sub.renew();
        let invoice = sub.generate_invoice(customer_name);
        let sub_clone = sub.clone();
        drop(subs);

        // Store the invoice.
        let invoice = self.create_invoice(invoice)?;

        self.emit("subscription.renewed", id);
        Ok((sub_clone, invoice))
    }

    /// Check all subscriptions for needed renewals and generate invoices.
    /// Returns the list of (subscription, new invoice) pairs.
    pub fn process_renewals(&self, customer_name_fn: impl Fn(&str) -> String) -> Vec<(Subscription, Invoice)> {
        let ids: Vec<String> = self
            .subscriptions
            .read()
            .values()
            .filter(|s| s.needs_renewal())
            .map(|s| s.id.clone())
            .collect();

        let mut results = Vec::new();
        for id in ids {
            let customer_id = match self.get_subscription(&id) {
                Some(s) => s.customer_id.clone(),
                None => continue,
            };
            let name = customer_name_fn(&customer_id);
            if let Ok((sub, inv)) = self.renew_subscription(&id, &name) {
                results.push((sub, inv));
            }
        }
        results
    }

    fn emit(&self, name: &str, id: &str) {
        if let Some(bus) = &self.event_bus {
            bus.publish(name, id.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> BillingStore {
        let bus = Arc::new(nexora_core::EventBus::new());
        BillingStore::new().with_event_bus(bus)
    }

    fn sample_invoice(customer: &str, total: u64) -> Invoice {
        let mut inv = Invoice::new_draft(customer, "Test", "USD");
        inv.add_item(crate::types::InvoiceItem {
            description: "Test item".into(),
            package_id: None,
            quantity: 1,
            unit_price_minor: total,
            total_minor: total,
            currency: "USD".into(),
        });
        inv.mark_open();
        inv
    }

    #[test]
    fn create_and_get_invoice() {
        let store = setup();
        let inv = store.create_invoice(sample_invoice("u1", 1000)).unwrap();
        assert!(store.get_invoice(&inv.id).is_some());
        assert_eq!(store.invoice_count(), 1);
    }

    #[test]
    fn list_invoices_for_customer() {
        let store = setup();
        store.create_invoice(sample_invoice("u1", 1000)).unwrap();
        store.create_invoice(sample_invoice("u1", 2000)).unwrap();
        store.create_invoice(sample_invoice("u2", 500)).unwrap();
        assert_eq!(store.list_invoices_for_customer("u1").len(), 2);
        assert_eq!(store.list_invoices_for_customer("u2").len(), 1);
    }

    #[test]
    fn payment_succeeds_and_marks_invoice_paid() {
        let store = setup();
        let inv = store.create_invoice(sample_invoice("u1", 1000)).unwrap();
        let pay = store
            .create_payment(Payment::new_pending(&inv.id, "u1", 1000, "USD", "card"))
            .unwrap();
        let (pay, inv) = store.succeed_payment(&pay.id).unwrap();
        assert_eq!(pay.status, PaymentStatus::Succeeded);
        assert!(inv.is_some());
        assert_eq!(inv.unwrap().status, InvoiceStatus::Paid);
    }

    #[test]
    fn payment_wrong_amount_fails() {
        let store = setup();
        let inv = store.create_invoice(sample_invoice("u1", 1000)).unwrap();
        let pay = store
            .create_payment(Payment::new_pending(&inv.id, "u1", 500, "USD", "card"))
            .unwrap();
        let err = store.succeed_payment(&pay.id).unwrap_err();
        assert!(matches!(err, BillingError::AmountMismatch { .. }));
    }

    #[test]
    fn subscription_lifecycle() {
        let store = setup();
        let sub = Subscription::new_active("u1", "com.test.pkg", 999, "USD", 1); // 1 second period
        let sub = store.create_subscription(sub).unwrap();
        assert_eq!(store.subscription_count(), 1);

        // Wait for period to end.
        std::thread::sleep(std::time::Duration::from_millis(1100));

        // Process renewals.
        let renewals = store.process_renewals(|_| "Alice".into());
        assert_eq!(renewals.len(), 1);
        assert_eq!(renewals[0].1.total_minor, 999);

        // Cancel.
        let sub = store.cancel_subscription(&sub.id).unwrap();
        assert_eq!(sub.status, SubscriptionStatus::Cancelled);
    }

    #[test]
    fn void_invoice_works() {
        let store = setup();
        let inv = store.create_invoice(sample_invoice("u1", 1000)).unwrap();
        let inv = store.void_invoice(&inv.id).unwrap();
        assert_eq!(inv.status, InvoiceStatus::Void);
    }

    #[test]
    fn cannot_void_paid_invoice() {
        let store = setup();
        let inv = store.create_invoice(sample_invoice("u1", 1000)).unwrap();
        let pay = store
            .create_payment(Payment::new_pending(&inv.id, "u1", 1000, "USD", "card"))
            .unwrap();
        store.succeed_payment(&pay.id).unwrap();
        let err = store.void_invoice(&inv.id).unwrap_err();
        assert!(matches!(err, BillingError::InvoiceWrongState { .. }));
    }

    #[test]
    fn events_emitted() {
        let bus = Arc::new(nexora_core::EventBus::new());
        let store = BillingStore::new().with_event_bus(bus.clone());

        let inv = store.create_invoice(sample_invoice("u1", 1000)).unwrap();
        let pay = store
            .create_payment(Payment::new_pending(&inv.id, "u1", 1000, "USD", "card"))
            .unwrap();
        store.succeed_payment(&pay.id).unwrap();

        let events = bus.replay_filtered(0, "");
        let names: Vec<&str> = events.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"invoice.created"));
        assert!(names.contains(&"payment.created"));
        assert!(names.contains(&"payment.succeeded"));
        assert!(names.contains(&"invoice.paid"));
    }
}
