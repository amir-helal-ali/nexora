//! Notification types.

use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;

/// Unique notification ID.
pub type NotificationId = String;

/// Notification severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationSeverity {
    /// Informational.
    Info,
    /// Success.
    Success,
    /// Warning.
    Warning,
    /// Error / critical.
    Error,
}

impl fmt::Display for NotificationSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => f.write_str("info"),
            Self::Success => f.write_str("success"),
            Self::Warning => f.write_str("warning"),
            Self::Error => f.write_str("error"),
        }
    }
}

/// A notification for a user.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Notification {
    /// Unique notification ID.
    pub id: NotificationId,
    /// User ID this notification is for.
    pub user_id: String,
    /// Notification title.
    pub title: String,
    /// Notification body.
    pub body: String,
    /// Severity level.
    pub severity: NotificationSeverity,
    /// Whether the notification has been read.
    pub read: bool,
    /// When the notification was created (unix nanos).
    pub created_at: i64,
    /// Optional link (e.g. "/billing" to navigate to).
    pub link: Option<String>,
    /// Optional icon (emoji or name).
    pub icon: Option<String>,
}

impl Notification {
    /// Construct a new notification.
    pub fn new(user_id: &str, title: &str, body: &str, severity: NotificationSeverity) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            severity,
            read: false,
            created_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            link: None,
            icon: None,
        }
    }

    /// Set a link.
    pub fn with_link(mut self, link: &str) -> Self {
        self.link = Some(link.to_string());
        self
    }

    /// Set an icon.
    pub fn with_icon(mut self, icon: &str) -> Self {
        self.icon = Some(icon.to_string());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_construction() {
        let n = Notification::new("u1", "Welcome", "Hello!", NotificationSeverity::Info)
            .with_link("/dashboard")
            .with_icon("👋");
        assert_eq!(n.user_id, "u1");
        assert_eq!(n.severity, NotificationSeverity::Info);
        assert!(!n.read);
        assert_eq!(n.link, Some("/dashboard".into()));
        assert_eq!(n.icon, Some("👋".into()));
    }

    #[test]
    fn severity_display() {
        assert_eq!(NotificationSeverity::Info.to_string(), "info");
        assert_eq!(NotificationSeverity::Error.to_string(), "error");
    }
}
