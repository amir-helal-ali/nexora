//! خدمة فوترة Nexora — فواتير، مدفوعات، اشتراكات.
//!
//! أول وحدة توليد إيرادات في المنصة. تدعم نماذج الفوترة الخمسة المعرفة
//! في المتجر (الجزء 5):
//! - شراء لمرة واحدة
//! - اشتراك (متكرر)
//! - قائم على الاستخدام (لكل أمر NXP / لكل حدث)
//! - ترخيص مؤسسي (شروط مخصصة)
//! - مجاني
//!
//! # التكامل
//!
//! - تقرأ نماذج الفوترة من بيانات الحزم (المتجر)
//! - تنبعث أحداث: invoice.created، payment.completed، subscription.renewed،
//!   subscription.cancelled
//! - كل تغييرات الحالة قابلة للتدقيق عبر ناقل الأحداث

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod handler;
pub mod store;
pub mod types;

pub use handler::BillingHandler;
pub use store::BillingStore;
pub use types::{Invoice, InvoiceId, InvoiceItem, InvoiceStatus, Payment, PaymentId, PaymentStatus, Subscription, SubscriptionId, SubscriptionStatus};

use nexora_core::NexoraCore;
use std::sync::Arc;

/// The Billing service. Owns the billing store + references to Core.
pub struct BillingService {
    /// Billing store (in-memory + write-through to SQLite via v0.2).
    pub store: BillingStore,
    /// Reference to the Core (for events).
    pub core: Arc<NexoraCore>,
}

impl std::fmt::Debug for BillingService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BillingService")
            .field("invoices", &self.store.invoice_count())
            .field("payments", &self.store.payment_count())
            .field("subscriptions", &self.store.subscription_count())
            .field("core", &self.core)
            .finish()
    }
}

impl BillingService {
    /// Construct a new Billing service.
    pub fn new(core: Arc<NexoraCore>) -> Self {
        Self {
            store: BillingStore::new().with_event_bus(core.events_inner()),
            core,
        }
    }

    /// Returns a handler for dispatching billing commands.
    pub fn handler(self: Arc<Self>) -> BillingHandler {
        BillingHandler::new(self)
    }
}
