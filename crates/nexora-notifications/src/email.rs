//! Email adapter (SMTP via `lettre`).
//!
//! Sends transactional emails via SMTP. Supports TLS, authentication, and
//! multiple recipients (though Nexora notifications are 1:1).

use crate::channel::{Channel, ChannelKind};
use crate::error::{NotificationError, NotificationResult};
use crate::message::Notification;
use async_trait::async_trait;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};

/// Email adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    /// SMTP server hostname.
    pub host: String,
    /// SMTP port (587 for STARTTLS, 465 for implicit TLS).
    pub port: u16,
    /// Username for auth.
    pub username: String,
    /// Password for auth.
    pub password: String,
    /// From address (e.g. `Nexora <noreply@nexora.dev>`).
    pub from_address: String,
    /// Whether to use TLS.
    #[serde(default = "default_tls")]
    pub use_tls: bool,
}

fn default_tls() -> bool {
    true
}

/// Email adapter — sends emails via SMTP.
pub struct EmailAdapter {
    config: EmailConfig,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl EmailAdapter {
    /// Construct a new email adapter from config.
    pub fn new(config: EmailConfig) -> Self {
        let mut builder = if config.use_tls && config.port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
                .expect("invalid SMTP host")
                .port(config.port)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
                .expect("invalid SMTP host")
                .port(config.port)
        };
        builder = builder.credentials(Credentials::new(
            config.username.clone(),
            config.password.clone(),
        ));
        Self {
            config,
            transport: builder.build(),
        }
    }

    /// Construct a mock adapter that doesn't actually send (for testing).
    /// The transport will be configured to localhost:25 with no auth.
    pub fn mock() -> Self {
        let config = EmailConfig {
            host: "localhost".into(),
            port: 2525,
            username: String::new(),
            password: String::new(),
            from_address: "test@nexora.dev".into(),
            use_tls: false,
        };
        let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous("localhost")
            .port(2525)
            .build();
        Self { config, transport }
    }

    /// Build a `lettre::Message` from a notification.
    pub fn build_email(&self, n: &Notification) -> NotificationResult<Message> {
        let from = self
            .config
            .from_address
            .parse()
            .map_err(|e| NotificationError::InvalidRecipient(format!("bad from_address: {e}")))?;
        let to = n
            .recipient
            .parse()
            .map_err(|e| NotificationError::InvalidRecipient(format!("bad recipient: {e}")))?;
        let msg = Message::builder()
            .from(from)
            .to(to)
            .subject(&n.payload.title)
            .header(ContentType::TEXT_PLAIN)
            .body(n.payload.body.clone())
            .map_err(|e| NotificationError::Email(e.to_string()))?;
        Ok(msg)
    }

    /// Config accessor.
    pub fn config(&self) -> &EmailConfig {
        &self.config
    }
}

#[async_trait]
impl Channel for EmailAdapter {
    fn kind(&self) -> ChannelKind {
        ChannelKind::Email
    }

    fn name(&self) -> &str {
        "email-smtp"
    }

    async fn deliver(&self, n: &Notification) -> NotificationResult<()> {
        let email = self.build_email(n)?;
        self.transport
            .send(email)
            .await
            .map_err(|e| NotificationError::DeliveryFailed(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Notification, NotificationPayload};

    fn sample_config() -> EmailConfig {
        EmailConfig {
            host: "smtp.example.com".into(),
            port: 587,
            username: "user".into(),
            password: "pass".into(),
            from_address: "Nexora <noreply@nexora.dev>".into(),
            use_tls: true,
        }
    }

    #[test]
    fn build_email_extracts_fields() {
        let adapter = EmailAdapter::new(sample_config());
        let n = Notification::new(
            "u1",
            "alice@example.com",
            "email",
            NotificationPayload::new("Welcome to Nexora", "Hello Alice, welcome!"),
        );
        let email = adapter.build_email(&n).unwrap();
        // lettre Message::formatted() returns the full RFC 822 message bytes.
        let bytes = email.formatted();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("Welcome to Nexora"));
        assert!(s.contains("Hello Alice"));
    }

    #[test]
    fn build_email_rejects_invalid_recipient() {
        let adapter = EmailAdapter::new(sample_config());
        let n = Notification::new(
            "u1",
            "not an email",
            "email",
            NotificationPayload::new("x", "y"),
        );
        assert!(adapter.build_email(&n).is_err());
    }

    #[test]
    fn config_serde_roundtrip() {
        let cfg = sample_config();
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: EmailConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.host, cfg2.host);
        assert_eq!(cfg.port, cfg2.port);
        assert_eq!(cfg.use_tls, cfg2.use_tls);
    }

    #[test]
    fn mock_adapter_works() {
        let _adapter = EmailAdapter::mock();
    }

    #[test]
    fn channel_kind_is_email() {
        let adapter = EmailAdapter::new(sample_config());
        assert_eq!(adapter.kind(), ChannelKind::Email);
        assert_eq!(adapter.name(), "email-smtp");
    }
}
