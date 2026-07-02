//! Channel abstraction — the trait every notification adapter implements.

use crate::error::NotificationResult;
use crate::message::Notification;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// The kind of channel (for diagnostics and routing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    Email,
    WebPush,
    InApp,
    Sms,
    Slack,
    Webhook,
}

impl ChannelKind {
    /// Stable string identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::WebPush => "webpush",
            Self::InApp => "in_app",
            Self::Sms => "sms",
            Self::Slack => "slack",
            Self::Webhook => "webhook",
        }
    }
}

/// Delivery status of a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    /// Notification created, not yet dispatched.
    Pending,
    /// Currently being delivered.
    InFlight,
    /// Successfully delivered.
    Delivered,
    /// Delivery failed (terminal).
    Failed,
    /// Delivery failed but will be retried.
    RetryScheduled,
    /// User opted out / unsubscribed.
    Suppressed,
}

impl std::fmt::Display for DeliveryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => f.write_str("pending"),
            Self::InFlight => f.write_str("in_flight"),
            Self::Delivered => f.write_str("delivered"),
            Self::Failed => f.write_str("failed"),
            Self::RetryScheduled => f.write_str("retry_scheduled"),
            Self::Suppressed => f.write_str("suppressed"),
        }
    }
}

/// A notification channel — a delivery adapter for one transport (email,
/// web push, etc.).
#[async_trait]
pub trait Channel: Send + Sync {
    /// The kind of this channel.
    fn kind(&self) -> ChannelKind;

    /// The name of this channel (e.g. "email-smtp", "webpush-vapid").
    fn name(&self) -> &str;

    /// Whether this channel is currently healthy and able to deliver.
    /// The dispatcher skips unhealthy channels.
    async fn is_healthy(&self) -> bool {
        true
    }

    /// Deliver a notification. Returns `Ok(())` on success, `Err` on failure.
    /// The dispatcher updates the notification's status based on the result.
    async fn deliver(&self, notification: &Notification) -> NotificationResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_kind_as_str() {
        assert_eq!(ChannelKind::Email.as_str(), "email");
        assert_eq!(ChannelKind::WebPush.as_str(), "webpush");
        assert_eq!(ChannelKind::InApp.as_str(), "in_app");
    }

    #[test]
    fn delivery_status_display() {
        assert_eq!(DeliveryStatus::Pending.to_string(), "pending");
        assert_eq!(DeliveryStatus::Delivered.to_string(), "delivered");
        assert_eq!(DeliveryStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn delivery_status_serde() {
        let s = serde_json::to_string(&DeliveryStatus::InFlight).unwrap();
        assert_eq!(s, "\"in_flight\"");
        let s2: DeliveryStatus = serde_json::from_str(&s).unwrap();
        assert_eq!(s2, DeliveryStatus::InFlight);
    }
}
