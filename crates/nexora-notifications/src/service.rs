//! Notification dispatcher — routes notifications to the right channel,
//! tracks delivery status, and emits audit events.

use crate::channel::{Channel, DeliveryStatus};
use crate::error::NotificationResult;
use crate::message::{Notification, NotificationId, NotificationPayload, Priority};
use crate::store::InAppStore;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use time::OffsetDateTime;

/// The notification service. Owns the channel registry and the in-app store.
pub struct NotificationService {
    channels: RwLock<HashMap<String, Arc<dyn Channel>>>,
    in_app: Arc<InAppStore>,
    /// Recent delivery log (for diagnostics). Capped at 1000 entries.
    log: RwLock<Vec<Notification>>,
    /// Optional event bus for audit trail.
    event_bus: Option<Arc<nexora_core::EventBus>>,
}

impl std::fmt::Debug for NotificationService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationService")
            .field("channels", &self.channels.read().len())
            .field("in_app_count", &self.in_app.count("__diag__"))
            .field("log_len", &self.log.read().len())
            .finish()
    }
}

impl NotificationService {
    /// Construct a new notification service with no channels registered.
    pub fn new() -> Self {
        Self {
            channels: RwLock::new(HashMap::new()),
            in_app: Arc::new(InAppStore::default()),
            log: RwLock::new(Vec::new()),
            event_bus: None,
        }
    }

    /// Construct with an event bus (for audit trail).
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Register a delivery channel.
    pub fn register_channel(&self, name: &str, channel: Arc<dyn Channel>) {
        self.channels.write().insert(name.to_string(), channel);
    }

    /// List registered channel names.
    pub fn channel_names(&self) -> Vec<String> {
        self.channels.read().keys().cloned().collect()
    }

    /// Access the in-app store (for SSE-driven dashboards).
    pub fn in_app_store(&self) -> &Arc<InAppStore> {
        &self.in_app
    }

    /// Send a notification via the specified channel.
    pub async fn send(&self, mut notification: Notification) -> NotificationResult<Notification> {
        let channel_name = notification.channel.clone();
        let channel = {
            let channels = self.channels.read();
            channels
                .get(&channel_name)
                .cloned()
                .ok_or_else(|| crate::error::NotificationError::ChannelNotConfigured(channel_name.clone()))?
        };

        notification.status = DeliveryStatus::InFlight;
        let result = channel.deliver(&notification).await;
        match result {
            Ok(()) => {
                notification.mark_delivered();
                self.emit_event("notification.delivered", &notification);
            }
            Err(e) => {
                notification.mark_failed(e.to_string());
                self.emit_event("notification.failed", &notification);
            }
        }

        // Append to log (cap at 1000).
        let mut log = self.log.write();
        log.push(notification.clone());
        if log.len() > 1000 {
            log.remove(0);
        }

        Ok(notification)
    }

    /// Convenience: send an in-app notification (no external channel needed).
    pub fn send_in_app(
        &self,
        user_id: &str,
        title: &str,
        body: &str,
        action_url: Option<String>,
    ) -> NotificationResult<crate::store::InAppNotification> {
        let n = self.in_app.add(user_id, title, body, action_url)?;
        self.emit_event_str("notification.in_app.added", user_id, &n.title);
        Ok(n)
    }

    /// Convenience: send an email notification.
    pub async fn send_email(
        &self,
        user_id: &str,
        to: &str,
        title: &str,
        body: &str,
    ) -> NotificationResult<Notification> {
        let n = Notification::new(user_id, to, "email", NotificationPayload::new(title, body));
        self.send(n).await
    }

    /// Convenience: send a web push notification.
    pub async fn send_webpush(
        &self,
        user_id: &str,
        subscription: &str,
        title: &str,
        body: &str,
    ) -> NotificationResult<Notification> {
        let n = Notification::new(user_id, subscription, "webpush", NotificationPayload::new(title, body));
        self.send(n).await
    }

    /// Get a notification from the delivery log.
    pub fn get_from_log(&self, id: &NotificationId) -> Option<Notification> {
        self.log.read().iter().find(|n| n.id == *id).cloned()
    }

    /// List recent deliveries (newest first).
    pub fn recent_deliveries(&self, limit: usize) -> Vec<Notification> {
        let log = self.log.read();
        log.iter().rev().take(limit).cloned().collect()
    }

    /// Count deliveries by status.
    pub fn count_by_status(&self, status: DeliveryStatus) -> usize {
        self.log.read().iter().filter(|n| n.status == status).count()
    }

    fn emit_event(&self, name: &str, n: &Notification) {
        if let Some(bus) = &self.event_bus {
            bus.publish(name, n.id.clone());
        }
    }

    fn emit_event_str(&self, name: &str, user_id: &str, title: &str) {
        if let Some(bus) = &self.event_bus {
            bus.publish(name, format!("{user_id}:{title}"));
        }
    }
}

impl Default for NotificationService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A test channel that records all deliveries.
    struct TestChannel {
        name: String,
        deliveries: Arc<AtomicU32>,
        should_fail: bool,
    }

    #[async_trait]
    impl Channel for TestChannel {
        fn kind(&self) -> crate::channel::ChannelKind {
            crate::channel::ChannelKind::InApp
        }
        fn name(&self) -> &str {
            &self.name
        }
        async fn deliver(&self, _n: &Notification) -> NotificationResult<()> {
            self.deliveries.fetch_add(1, Ordering::SeqCst);
            if self.should_fail {
                Err(crate::error::NotificationError::DeliveryFailed("test failure".into()))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn send_delivers_via_registered_channel() {
        let svc = NotificationService::new();
        let counter = Arc::new(AtomicU32::new(0));
        let ch = Arc::new(TestChannel {
            name: "test".into(),
            deliveries: counter.clone(),
            should_fail: false,
        });
        svc.register_channel("test", ch);

        let n = Notification::new("u1", "r1", "test", NotificationPayload::new("T", "B"));
        let result = svc.send(n).await.unwrap();
        assert_eq!(result.status, DeliveryStatus::Delivered);
        assert!(result.delivered_at.is_some());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn send_records_failed_delivery() {
        let svc = NotificationService::new();
        let ch = Arc::new(TestChannel {
            name: "fail".into(),
            deliveries: Arc::new(AtomicU32::new(0)),
            should_fail: true,
        });
        svc.register_channel("fail", ch);

        let n = Notification::new("u1", "r1", "fail", NotificationPayload::new("T", "B"));
        let result = svc.send(n).await.unwrap();
        assert_eq!(result.status, DeliveryStatus::Failed);
        assert!(result.error.as_ref().unwrap().contains("test failure"));
        assert_eq!(result.attempts, 1);
    }

    #[tokio::test]
    async fn send_fails_for_unregistered_channel() {
        let svc = NotificationService::new();
        let n = Notification::new("u1", "r1", "nonexistent", NotificationPayload::new("T", "B"));
        assert!(svc.send(n).await.is_err());
    }

    #[tokio::test]
    async fn log_capped_at_1000_entries() {
        let svc = NotificationService::new();
        let ch = Arc::new(TestChannel {
            name: "test".into(),
            deliveries: Arc::new(AtomicU32::new(0)),
            should_fail: false,
        });
        svc.register_channel("test", ch);

        for _ in 0..1050 {
            let n = Notification::new("u1", "r1", "test", NotificationPayload::new("T", "B"));
            svc.send(n).await.unwrap();
        }
        let log = svc.log.read();
        assert_eq!(log.len(), 1000);
    }

    #[test]
    fn send_in_app_writes_to_store() {
        let svc = NotificationService::new();
        svc.send_in_app("u1", "T", "B", None).unwrap();
        assert_eq!(svc.in_app_store().count("u1"), 1);
    }

    #[tokio::test]
    async fn send_email_convenience_method() {
        let svc = NotificationService::new();
        let counter = Arc::new(AtomicU32::new(0));
        let ch = Arc::new(TestChannel {
            name: "email".into(),
            deliveries: counter.clone(),
            should_fail: false,
        });
        svc.register_channel("email", ch);

        let result = svc.send_email("u1", "a@b.c", "T", "B").await.unwrap();
        assert_eq!(result.channel, "email");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn send_webpush_convenience_method() {
        let svc = NotificationService::new();
        let counter = Arc::new(AtomicU32::new(0));
        let ch = Arc::new(TestChannel {
            name: "webpush".into(),
            deliveries: counter.clone(),
            should_fail: false,
        });
        svc.register_channel("webpush", ch);

        let result = svc.send_webpush("u1", "ep|k|a", "T", "B").await.unwrap();
        assert_eq!(result.channel, "webpush");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn channel_names_returns_registered() {
        let svc = NotificationService::new();
        let ch = Arc::new(TestChannel {
            name: "x".into(),
            deliveries: Arc::new(AtomicU32::new(0)),
            should_fail: false,
        });
        svc.register_channel("x", ch);
        let names = svc.channel_names();
        assert_eq!(names, vec!["x"]);
    }

    #[tokio::test]
    async fn recent_deliveries_returns_newest_first() {
        let svc = NotificationService::new();
        let ch = Arc::new(TestChannel {
            name: "test".into(),
            deliveries: Arc::new(AtomicU32::new(0)),
            should_fail: false,
        });
        svc.register_channel("test", ch);

        for i in 0..5 {
            let n = Notification::new("u1", "r1", "test", NotificationPayload::new(format!("T{i}"), "B"));
            svc.send(n).await.unwrap();
        }
        let recent = svc.recent_deliveries(3);
        assert_eq!(recent.len(), 3);
        // Newest first means T4, T3, T2.
        assert_eq!(recent[0].payload.title, "T4");
        assert_eq!(recent[1].payload.title, "T3");
        assert_eq!(recent[2].payload.title, "T2");
    }

    #[tokio::test]
    async fn count_by_status() {
        let svc = NotificationService::new();
        let ch_ok = Arc::new(TestChannel {
            name: "ok".into(),
            deliveries: Arc::new(AtomicU32::new(0)),
            should_fail: false,
        });
        let ch_fail = Arc::new(TestChannel {
            name: "fail".into(),
            deliveries: Arc::new(AtomicU32::new(0)),
            should_fail: true,
        });
        svc.register_channel("ok", ch_ok);
        svc.register_channel("fail", ch_fail);

        svc.send(Notification::new("u", "r", "ok", NotificationPayload::new("T", "B")))
            .await
            .unwrap();
        svc.send(Notification::new("u", "r", "fail", NotificationPayload::new("T", "B")))
            .await
            .unwrap();
        svc.send(Notification::new("u", "r", "ok", NotificationPayload::new("T", "B")))
            .await
            .unwrap();

        assert_eq!(svc.count_by_status(DeliveryStatus::Delivered), 2);
        assert_eq!(svc.count_by_status(DeliveryStatus::Failed), 1);
    }

    #[tokio::test]
    async fn emits_events_on_bus() {
        let bus = Arc::new(nexora_core::EventBus::new());
        let sub = bus.subscribe("");
        let svc = NotificationService::new().with_event_bus(bus.clone());
        let ch = Arc::new(TestChannel {
            name: "test".into(),
            deliveries: Arc::new(AtomicU32::new(0)),
            should_fail: false,
        });
        svc.register_channel("test", ch);

        let n = Notification::new("u", "r", "test", NotificationPayload::new("T", "B"));
        svc.send(n).await.unwrap();

        // Give the subscriber a moment to receive.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(bus.published_count() >= 1);
    }
}
