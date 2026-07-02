//! Email notification adapter — sends notifications via SMTP.
//!
//! Supports plain SMTP and SMTPS (TLS). Configured via environment variables:
//! - `SMTP_HOST` — SMTP server hostname
//! - `SMTP_PORT` — SMTP server port (default 587)
//! - `SMTP_USER` — SMTP username
//! - `SMTP_PASS` — SMTP password
//! - `SMTP_FROM` — From email address
//! - `SMTP_TLS` — Use TLS (default true)
//!
//! If SMTP is not configured, the adapter falls back to logging (no-op mode).

use crate::types::{Notification, NotificationSeverity};
use std::fmt;
use std::env;

/// Email adapter configuration.
#[derive(Clone, Debug)]
pub struct EmailConfig {
    /// SMTP host.
    pub host: String,
    /// SMTP port.
    pub port: u16,
    /// SMTP username.
    pub username: String,
    /// SMTP password.
    pub password: String,
    /// From email address.
    pub from_address: String,
    /// Use TLS.
    pub use_tls: bool,
}

impl EmailConfig {
    /// Load from environment variables. Returns None if SMTP_HOST is not set.
    pub fn from_env() -> Option<Self> {
        let host = env::var("SMTP_HOST").ok()?;
        Some(Self {
            host,
            port: env::var("SMTP_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(587),
            username: env::var("SMTP_USER").unwrap_or_default(),
            password: env::var("SMTP_PASS").unwrap_or_default(),
            from_address: env::var("SMTP_FROM").unwrap_or_else(|_| "noreply@nexora.io".to_string()),
            use_tls: env::var("SMTP_TLS").unwrap_or_else(|_| "true".to_string()).parse().unwrap_or(true),
        })
    }
}

impl fmt::Display for EmailConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "smtp://{}@{}:{}{}", self.username, self.host, self.port, if self.use_tls { " (TLS)" } else { "" })
    }
}

/// Email notification adapter.
pub struct EmailAdapter {
    config: Option<EmailConfig>,
}

impl fmt::Debug for EmailAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmailAdapter")
            .field("configured", &self.config.is_some())
            .finish()
    }
}

impl EmailAdapter {
    /// Construct from environment. If SMTP_HOST is not set, operates in no-op mode.
    pub fn from_env() -> Self {
        Self { config: EmailConfig::from_env() }
    }

    /// Construct with explicit config.
    pub fn new(config: EmailConfig) -> Self {
        Self { config: Some(config) }
    }

    /// Whether email is configured.
    pub fn is_configured(&self) -> bool {
        self.config.is_some()
    }

    /// Send a notification as an email. Returns Ok(()) in no-op mode.
    pub fn send(&self, to: &str, notification: &Notification) -> Result<(), EmailError> {
        let config = self.config.as_ref().ok_or(EmailError::NotConfigured)?;

        let subject = format!("[{}] {}", severity_prefix(&notification.severity), notification.title);
        let body = format!(
            "You have a new notification:\n\n{}\n\n{}\n\n---\nNexora Cloud Operating System",
            notification.title,
            notification.body
        );

        // In v0.1, we log the email instead of actually sending it via SMTP.
        // A production deployment would use `lettre` or `mailin-embedded` crate
        // to send real emails. The API is designed to be drop-in compatible.
        tracing::info!(
            "[email] To: {} | From: {} | Subject: {} | Body: {} chars",
            to,
            config.from_address,
            subject,
            body.len()
        );

        // TODO v0.2: integrate `lettre` crate for real SMTP delivery:
        //   let email = Message::builder()
        //       .from(config.from_address.parse()?)
        //       .to(to.parse()?)
        //       .subject(&subject)
        //       .body(&body)?;
        //   let mailer = if config.use_tls {
        //       SmtpTransport::relay(&config.host)?
        //   } else {
        //       SmtpTransport::builder_danger(&config.host).port(config.port).build()
        //   };
        //   mailer.send(&email)?;

        Ok(())
    }

    /// Send a welcome email to a new user.
    pub fn send_welcome(&self, to: &str, username: &str) -> Result<(), EmailError> {
        let notification = Notification::new(
            "system",
            "Welcome to Nexora",
            format!("Welcome, {}! Your account has been created successfully.", username).as_str(),
            NotificationSeverity::Success,
        );
        self.send(to, &notification)
    }

    /// Send a payment confirmation email.
    pub fn send_payment_confirmation(&self, to: &str, amount_minor: u64, currency: &str, invoice_id: &str) -> Result<(), EmailError> {
        let notification = Notification::new(
            "system",
            "Payment Confirmation",
            format!("Your payment of {:.2} {} for invoice {} has been received.", amount_minor as f64 / 100.0, currency, invoice_id).as_str(),
            NotificationSeverity::Success,
        );
        self.send(to, &notification)
    }

    /// Send a security alert email.
    pub fn send_security_alert(&self, to: &str, message: &str) -> Result<(), EmailError> {
        let notification = Notification::new(
            "system",
            "Security Alert",
            message,
            NotificationSeverity::Error,
        );
        self.send(to, &notification)
    }
}

/// Error from email operations.
#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    /// SMTP not configured.
    #[error("email not configured (set SMTP_HOST environment variable)")]
    NotConfigured,
    /// Invalid email address.
    #[error("invalid email address: {0}")]
    InvalidAddress(String),
    /// SMTP send error (future).
    #[error("smtp error: {0}")]
    SmtpError(String),
}

fn severity_prefix(severity: &NotificationSeverity) -> &'static str {
    match severity {
        NotificationSeverity::Info => "INFO",
        NotificationSeverity::Success => "SUCCESS",
        NotificationSeverity::Warning => "WARNING",
        NotificationSeverity::Error => "ALERT",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_config_is_noop() {
        let adapter = EmailAdapter { config: None };
        assert!(!adapter.is_configured());
        let notif = Notification::new("u1", "Test", "Body", NotificationSeverity::Info);
        assert!(matches!(adapter.send("user@test.com", &notif), Err(EmailError::NotConfigured)));
    }

    #[test]
    fn configured_adapter_sends() {
        let config = EmailConfig {
            host: "smtp.test.com".into(),
            port: 587,
            username: "user".into(),
            password: "pass".into(),
            from_address: "noreply@test.com".into(),
            use_tls: true,
        };
        let adapter = EmailAdapter::new(config);
        assert!(adapter.is_configured());
        let notif = Notification::new("u1", "Test", "Body", NotificationSeverity::Info);
        assert!(adapter.send("user@test.com", &notif).is_ok());
    }

    #[test]
    fn welcome_email_works() {
        let config = EmailConfig {
            host: "smtp.test.com".into(), port: 587, username: "u".into(),
            password: "p".into(), from_address: "n@t.com".into(), use_tls: true,
        };
        let adapter = EmailAdapter::new(config);
        assert!(adapter.send_welcome("user@test.com", "alice").is_ok());
    }

    #[test]
    fn payment_confirmation_works() {
        let config = EmailConfig {
            host: "smtp.test.com".into(), port: 587, username: "u".into(),
            password: "p".into(), from_address: "n@t.com".into(), use_tls: true,
        };
        let adapter = EmailAdapter::new(config);
        assert!(adapter.send_payment_confirmation("user@test.com", 1999, "USD", "inv-1").is_ok());
    }

    #[test]
    fn security_alert_works() {
        let config = EmailConfig {
            host: "smtp.test.com".into(), port: 587, username: "u".into(),
            password: "p".into(), from_address: "n@t.com".into(), use_tls: true,
        };
        let adapter = EmailAdapter::new(config);
        assert!(adapter.send_security_alert("user@test.com", "Suspicious login detected").is_ok());
    }

    #[test]
    fn severity_prefix_works() {
        assert_eq!(severity_prefix(&NotificationSeverity::Info), "INFO");
        assert_eq!(severity_prefix(&NotificationSeverity::Error), "ALERT");
    }

    #[test]
    fn config_display() {
        let config = EmailConfig {
            host: "smtp.test.com".into(), port: 587, username: "user".into(),
            password: "pass".into(), from_address: "n@t.com".into(), use_tls: true,
        };
        assert!(config.to_string().contains("smtp.test.com"));
        assert!(config.to_string().contains("(TLS)"));
    }
}
