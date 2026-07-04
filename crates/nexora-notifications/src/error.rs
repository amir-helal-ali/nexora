//! Notification error types.

use thiserror::Error;

pub type NotificationResult<T> = Result<T, NotificationError>;

#[derive(Debug, Error)]
pub enum NotificationError {
    #[error("channel not configured: {0}")]
    ChannelNotConfigured(String),

    #[error("channel unavailable: {0}")]
    ChannelUnavailable(String),

    #[error("invalid recipient: {0}")]
    InvalidRecipient(String),

    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    #[error("delivery failed: {0}")]
    DeliveryFailed(String),

    #[error("email error: {0}")]
    #[cfg(feature = "email")]
    Email(String),

    #[error("web push error: {0}")]
    #[cfg(feature = "webpush")]
    WebPush(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("notification not found: {0}")]
    NotFound(String),

    #[error("serialization error: {0}")]
    Serde(String),
}

#[cfg(feature = "email")]
impl From<lettre::transport::smtp::Error> for NotificationError {
    fn from(e: lettre::transport::smtp::Error) -> Self {
        NotificationError::Email(e.to_string())
    }
}

#[cfg(feature = "email")]
impl From<lettre::error::Error> for NotificationError {
    fn from(e: lettre::error::Error) -> Self {
        NotificationError::Email(e.to_string())
    }
}

impl From<serde_json::Error> for NotificationError {
    fn from(e: serde_json::Error) -> Self {
        NotificationError::Serde(e.to_string())
    }
}
