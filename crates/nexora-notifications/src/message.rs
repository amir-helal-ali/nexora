//! Notification message model.

use crate::channel::DeliveryStatus;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// Unique notification ID.
pub type NotificationId = String;

/// Notification priority. Higher-priority notifications are dispatched first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Lowest priority — informational only.
    Low,
    /// Default priority.
    Normal,
    /// High priority — should be delivered ASAP.
    High,
    /// Urgent — system-critical (e.g. security alert).
    Urgent,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => f.write_str("low"),
            Self::Normal => f.write_str("normal"),
            Self::High => f.write_str("high"),
            Self::Urgent => f.write_str("urgent"),
        }
    }
}

/// The payload of a notification. Channel-agnostic — adapters translate
/// this into channel-specific formats (e.g. HTML email, push JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPayload {
    /// Short title (e.g. "Invoice #1234 paid").
    pub title: String,
    /// Body / message text (plain text).
    pub body: String,
    /// Optional URL to navigate to when clicked.
    #[serde(default)]
    pub action_url: Option<String>,
    /// Optional icon URL.
    #[serde(default)]
    pub icon_url: Option<String>,
    /// Optional tag (for grouping on the client).
    #[serde(default)]
    pub tag: Option<String>,
    /// Arbitrary key-value metadata.
    #[serde(default)]
    pub data: std::collections::HashMap<String, String>,
}

impl NotificationPayload {
    /// Construct a minimal payload with title + body.
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            action_url: None,
            icon_url: None,
            tag: None,
            data: std::collections::HashMap::new(),
        }
    }
}

/// A notification to be dispatched. Carries the recipient, payload, and
/// channel routing information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Unique notification ID.
    pub id: NotificationId,
    /// Recipient user ID.
    pub user_id: String,
    /// Recipient email/endpoint (channel-specific).
    pub recipient: String,
    /// Channel to deliver via.
    pub channel: String,
    /// Payload.
    pub payload: NotificationPayload,
    /// Priority.
    pub priority: Priority,
    /// When the notification was created (unix nanos).
    pub created_at: i64,
    /// When the notification was delivered (unix nanos), if successful.
    #[serde(default)]
    pub delivered_at: Option<i64>,
    /// Current delivery status.
    pub status: DeliveryStatus,
    /// Error message (if delivery failed).
    #[serde(default)]
    pub error: Option<String>,
    /// Number of delivery attempts.
    #[serde(default)]
    pub attempts: u32,
}

impl Notification {
    /// Construct a new notification with a fresh ID and `Pending` status.
    pub fn new(
        user_id: impl Into<String>,
        recipient: impl Into<String>,
        channel: impl Into<String>,
        payload: NotificationPayload,
    ) -> Self {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            recipient: recipient.into(),
            channel: channel.into(),
            payload,
            priority: Priority::default(),
            created_at: now,
            delivered_at: None,
            status: DeliveryStatus::Pending,
            error: None,
            attempts: 0,
        }
    }

    /// Set the priority.
    pub fn with_priority(mut self, p: Priority) -> Self {
        self.priority = p;
        self
    }

    /// Mark as delivered.
    pub fn mark_delivered(&mut self) {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        self.delivered_at = Some(now);
        self.status = DeliveryStatus::Delivered;
        self.error = None;
    }

    /// Mark as failed.
    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.attempts += 1;
        self.status = DeliveryStatus::Failed;
        self.error = Some(error.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_notification_has_pending_status() {
        let n = Notification::new("user-1", "alice@example.com", "email", NotificationPayload::new("Hi", "Hello"));
        assert_eq!(n.status, DeliveryStatus::Pending);
        assert_eq!(n.priority, Priority::Normal);
        assert_eq!(n.attempts, 0);
        assert!(!n.id.is_empty());
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::Urgent > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
    }

    #[test]
    fn mark_delivered_sets_status() {
        let mut n = Notification::new("u", "a@b.c", "email", NotificationPayload::new("x", "y"));
        n.mark_delivered();
        assert_eq!(n.status, DeliveryStatus::Delivered);
        assert!(n.delivered_at.is_some());
        assert!(n.error.is_none());
    }

    #[test]
    fn mark_failed_increments_attempts() {
        let mut n = Notification::new("u", "a@b.c", "email", NotificationPayload::new("x", "y"));
        n.mark_failed("timeout");
        assert_eq!(n.attempts, 1);
        assert_eq!(n.status, DeliveryStatus::Failed);
        assert_eq!(n.error, Some("timeout".into()));

        n.mark_failed("still timeout");
        assert_eq!(n.attempts, 2);
    }

    #[test]
    fn payload_new_minimal() {
        let p = NotificationPayload::new("Title", "Body");
        assert_eq!(p.title, "Title");
        assert_eq!(p.body, "Body");
        assert!(p.action_url.is_none());
        assert!(p.data.is_empty());
    }

    #[test]
    fn priority_serde_roundtrip() {
        let p = Priority::Urgent;
        let s = serde_json::to_string(&p).unwrap();
        assert_eq!(s, "\"urgent\"");
        let p2: Priority = serde_json::from_str(&s).unwrap();
        assert_eq!(p, p2);
    }

    #[test]
    fn notification_serde_roundtrip() {
        let n = Notification::new("u", "a@b.c", "email", NotificationPayload::new("x", "y"))
            .with_priority(Priority::High);
        let json = serde_json::to_string(&n).unwrap();
        let n2: Notification = serde_json::from_str(&json).unwrap();
        assert_eq!(n.id, n2.id);
        assert_eq!(n.priority, n2.priority);
        assert_eq!(n.recipient, n2.recipient);
    }
}
