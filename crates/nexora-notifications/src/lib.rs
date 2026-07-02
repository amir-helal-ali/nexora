//! Nexora Notification Service — user notifications, alerts, badges.
//!
//! Provides per-user notifications with read/unread tracking, severity levels,
//! and auto-generation from EventBus events.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod email;
pub mod handler;
pub mod store;
pub mod types;

pub use email::{EmailAdapter, EmailConfig, EmailError};
pub use handler::NotificationHandler;
pub use store::{NotificationError, NotificationStore};
pub use types::{Notification, NotificationId, NotificationSeverity};

use nexora_core::NexoraCore;
use std::sync::Arc;

/// The Notification service.
pub struct NotificationService {
    /// Notification store.
    pub store: NotificationStore,
    /// Reference to the Core.
    pub core: Arc<NexoraCore>,
}

impl std::fmt::Debug for NotificationService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationService")
            .field("notifications", &self.store.count())
            .finish()
    }
}

impl NotificationService {
    /// Construct a new Notification service.
    pub fn new(core: Arc<NexoraCore>) -> Self {
        let store = NotificationStore::new().with_event_bus(core.events_inner());
        Self { store, core }
    }

    /// Returns a handler.
    pub fn handler(self: Arc<Self>) -> NotificationHandler {
        NotificationHandler::new(self)
    }
}
