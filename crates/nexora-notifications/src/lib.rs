//! # خدمة إشعارات Nexora
//!
//! تسليم إشعارات متعدد القنوات لمنصة Nexora.
//!
//! ## القنوات
//!
//! - **البريد** (SMTP عبر `lettre`) — للرسائل المعاملاتية (ترحيب،
//!   إعادة تعيين كلمة المرور، إيصالات الفواتير).
//! - **دفع الويب** (RFC 8291 + VAPID JWT) — لإشعارات الدفع للمتصفح
//!   للأجهزة المشتركة.
//! - **داخل التطبيق** — للإشعارات الظاهرة في لوحة تحكم Nexora
//!   (مخزّنة في الذاكرة، تُجلب عبر SSE).
//!
//! ## البنية المعمارية
//!
//! ```text
//! +-----------+     +---------------------+     +----------+
//! | خدمة      |---->| خدمة الإشعارات      |---->| بريد     |
//! | (مثلاً     |     | (الموزّع)            |     | محول     |
//! |  فوترة)   |     |                     |---->+----------+
//! +-----------+     |                     |---->| دفع ويب  |
//!                   |                     |     | محول     |
//!                   |                     |---->+----------+
//!                   |                     |---->| تطبيق    |
//!                   |                     |     | محول     |
//!                   +---------------------+     +----------+
//!                          |
//!                          v
//!                   +---------------+
//!                   | ناقل الأحداث  |
//!                   | (مسار تدقيق)  |
//!                   +---------------+
//! ```
//!
//! كل إشعار — ناجح أو فاشل — ينبعث كحدث على ناقل الأحداث حتى يمكن
//! تدقيقه وإعادة تشغيله.

pub mod channel;
pub mod error;
pub mod message;
pub mod service;
pub mod store;

#[cfg(feature = "email")]
pub mod email;

#[cfg(feature = "webpush")]
pub mod webpush;

#[cfg(feature = "sms")]
pub mod sms;

#[cfg(feature = "slack")]
pub mod slack;

pub use channel::{Channel, ChannelKind, DeliveryStatus};
pub use error::{NotificationError, NotificationResult};
pub use message::{Notification, NotificationId, NotificationPayload, Priority};
pub use service::NotificationService;
pub use store::{InAppStore, InAppNotification};

#[cfg(feature = "email")]
pub use email::{EmailAdapter, EmailConfig};

#[cfg(feature = "webpush")]
pub use webpush::{WebPushAdapter, WebPushConfig, WebPushSubscription};

#[cfg(feature = "sms")]
pub use sms::{SmsAdapter, SmsConfig};

#[cfg(feature = "slack")]
pub use slack::{SlackAdapter, SlackConfig};
