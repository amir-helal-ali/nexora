//! # Nexora Notifications Service
//!
//! Multi-channel notification delivery for the Nexora platform.
//!
//! ## Channels
//!
//! - **Email** (SMTP via `lettre`) — for transactional emails (welcome,
//!   password reset, invoice receipts).
//! - **Web Push** (RFC 8291 + VAPID JWT) — for browser push notifications
//!   to subscribed devices.
//! - **In-App** — for notifications shown in the Nexora dashboard
//!   (stored in memory, fetched via SSE).
//!
//! ## Architecture
//!
//! ```text
//! +-----------+     +---------------------+     +----------+
//! | Service   |---->| NotificationService |---->| Email    |
//! | (e.g.     |     | (dispatcher)        |     | Adapter  |
//! |  billing) |     |                     |---->+----------+
//! +-----------+     |                     |---->| Web Push |
//!                   |                     |     | Adapter  |
//!                   |                     |---->+----------+
//!                   |                     |---->| In-App   |
//!                   |                     |     | Adapter  |
//!                   +---------------------+     +----------+
//!                          |
//!                          v
//!                   +---------------+
//!                   | EventBus      |
//!                   | (audit trail) |
//!                   +---------------+
//! ```
//!
//! Every notification — successful or failed — emits an event on the
//! EventBus so it can be audited and replayed.

pub mod channel;
pub mod error;
pub mod message;
pub mod service;
pub mod store;

#[cfg(feature = "email")]
pub mod email;

#[cfg(feature = "webpush")]
pub mod webpush;

pub use channel::{Channel, ChannelKind, DeliveryStatus};
pub use error::{NotificationError, NotificationResult};
pub use message::{Notification, NotificationId, NotificationPayload, Priority};
pub use service::NotificationService;
pub use store::{InAppStore, InAppNotification};

#[cfg(feature = "email")]
pub use email::{EmailAdapter, EmailConfig};

#[cfg(feature = "webpush")]
pub use webpush::{WebPushAdapter, WebPushConfig, WebPushSubscription};
