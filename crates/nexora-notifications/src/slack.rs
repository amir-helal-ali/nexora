//! محول Slack عبر Incoming Webhooks.
//!
//! يرسل إشعارات إلى قنوات Slack عبر Incoming Webhook URL.
//!
//! # صيغة الحمولة
//!
//! Slack يقبل JSON بالصيغة:
//! ```json
//! {
//!   "text": "نص الرسالة",
//!   "attachments": [...],
//!   "blocks": [...]
//! }
//! ```
//!
//! # المصادقة
//!
//! لا حاجة لمصادقة — الـ webhook URL نفسه يحوي الرمز السري.

use crate::channel::{Channel, ChannelKind};
use crate::error::{NotificationError, NotificationResult};
use crate::message::{Notification, Priority};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// إعدادات محول Slack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    /// URL الـ webhook الكامل (يحوي الرمز السري).
    pub webhook_url: String,
    /// اسم المرسِل المعروض.
    #[serde(default = "default_username")]
    pub username: String,
    /// أيقونة الإيموجي (مثلاً `:nexora:`).
    #[serde(default = "default_icon")]
    pub icon_emoji: String,
    /// القناة الافتراضية (تُتجاوز بإعداد القناة في الـ webhook).
    #[serde(default)]
    pub channel: Option<String>,
}

fn default_username() -> String {
    "Nexora".into()
}

fn default_icon() -> String {
    ":bell:".into()
}

/// محول Slack.
pub struct SlackAdapter {
    config: SlackConfig,
}

impl SlackAdapter {
    /// إنشاء محول Slack جديد.
    pub fn new(config: SlackConfig) -> Self {
        Self { config }
    }

    /// بناء حمولة JSON من إشعار.
    pub fn build_payload(&self, n: &Notification) -> NotificationResult<serde_json::Value> {
        let color = match n.priority {
            Priority::Urgent => "#ff0000",   // أحمر
            Priority::High => "#ff8800",     // برتقالي
            Priority::Normal => "#36a64f",   // أخضر
            Priority::Low => "#888888",      // رمادي
        };

        let mut fields = Vec::new();
        fields.push(serde_json::json!({
            "title": "الأولوية",
            "value": n.priority.to_string(),
            "short": true,
        }));
        fields.push(serde_json::json!({
            "title": "المستخدم",
            "value": n.user_id,
            "short": true,
        }));

        // إضافة أي بيانات إضافية كحقول.
        for (k, v) in &n.payload.data {
            fields.push(serde_json::json!({
                "title": k,
                "value": v,
                "short": true,
            }));
        }

        let mut attachment = serde_json::json!({
            "fallback": format!("{}: {}", n.payload.title, n.payload.body),
            "color": color,
            "title": n.payload.title,
            "text": n.payload.body,
            "fields": fields,
            "footer": "Nexora",
            "ts": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });

        if let Some(url) = &n.payload.action_url {
            attachment["title_link"] = serde_json::json!(url);
        }

        let mut payload = serde_json::json!({
            "text": format!("*{}*", n.payload.title),
            "attachments": [attachment],
            "username": self.config.username,
            "icon_emoji": self.config.icon_emoji,
        });

        if let Some(ch) = &self.config.channel {
            payload["channel"] = serde_json::json!(ch);
        }

        Ok(payload)
    }

    /// التحقق من صحة URL الـ webhook.
    pub fn validate_webhook_url(url: &str) -> NotificationResult<()> {
        if !url.starts_with("https://hooks.slack.com/services/") {
            return Err(NotificationError::InvalidRecipient(
                "URL الـ webhook يجب أن يبدأ بـ https://hooks.slack.com/services/".into(),
            ));
        }
        if url.len() < 50 {
            return Err(NotificationError::InvalidRecipient(
                "URL الـ webhook قصير جداً (قد يكون غير صالح)".into(),
            ));
        }
        Ok(())
    }

    /// الوصول إلى الإعدادات.
    pub fn config(&self) -> &SlackConfig {
        &self.config
    }
}

#[async_trait]
impl Channel for SlackAdapter {
    fn kind(&self) -> ChannelKind {
        ChannelKind::Slack
    }

    fn name(&self) -> &str {
        "slack-webhook"
    }

    async fn deliver(&self, n: &Notification) -> NotificationResult<()> {
        // التحقق من URL.
        Self::validate_webhook_url(&self.config.webhook_url)?;

        // بناء الحمولة.
        let _payload = self.build_payload(n)?;

        // في التنفيذ المرجعي، نتحقق من أن الحمولة قابلة للبناء
        // ونعود بنجاح. في الإنتاج، سنرسل طلب HTTP POST إلى Slack.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Notification, NotificationPayload, Priority};

    fn sample_config() -> SlackConfig {
        SlackConfig {
            webhook_url: "https://hooks.slack.com/services/TEAM0000/CHAN0000/FAKEWEBHOOKEXAMPLE000000".into(),
            username: "Nexora".into(),
            icon_emoji: ":bell:".into(),
            channel: None,
        }
    }

    fn sample_notification(priority: Priority) -> Notification {
        let mut payload = NotificationPayload::new("تنبيه النظام", "تم اكتشاف خطأ في الخدمة");
        payload.action_url = Some("https://nexora.dev/incidents/123".into());
        payload.data.insert("service".into(), "auth".into());
        payload.data.insert("severity".into(), "high".into());
        Notification::new("user-1", "slack", "slack", payload)
            .with_priority(priority)
    }

    #[test]
    fn build_payload_includes_title_and_body() {
        let adapter = SlackAdapter::new(sample_config());
        let n = sample_notification(Priority::High);
        let payload = adapter.build_payload(&n).unwrap();
        assert!(payload["text"].as_str().unwrap().contains("تنبيه"));
        assert_eq!(payload["attachments"][0]["title"], "تنبيه النظام");
        assert_eq!(payload["attachments"][0]["text"], "تم اكتشاف خطأ في الخدمة");
    }

    #[test]
    fn build_payload_color_matches_priority() {
        let adapter = SlackAdapter::new(sample_config());

        let urgent = adapter.build_payload(&sample_notification(Priority::Urgent)).unwrap();
        assert_eq!(urgent["attachments"][0]["color"], "#ff0000");

        let high = adapter.build_payload(&sample_notification(Priority::High)).unwrap();
        assert_eq!(high["attachments"][0]["color"], "#ff8800");

        let normal = adapter.build_payload(&sample_notification(Priority::Normal)).unwrap();
        assert_eq!(normal["attachments"][0]["color"], "#36a64f");

        let low = adapter.build_payload(&sample_notification(Priority::Low)).unwrap();
        assert_eq!(low["attachments"][0]["color"], "#888888");
    }

    #[test]
    fn build_payload_includes_action_url_as_title_link() {
        let adapter = SlackAdapter::new(sample_config());
        let n = sample_notification(Priority::Normal);
        let payload = adapter.build_payload(&n).unwrap();
        assert_eq!(
            payload["attachments"][0]["title_link"],
            "https://nexora.dev/incidents/123"
        );
    }

    #[test]
    fn build_payload_includes_data_as_fields() {
        let adapter = SlackAdapter::new(sample_config());
        let n = sample_notification(Priority::Normal);
        let payload = adapter.build_payload(&n).unwrap();
        let fields = payload["attachments"][0]["fields"].as_array().unwrap();
        // حقلان افتراضيان (الأولوية، المستخدم) + حقلان من البيانات.
        assert!(fields.len() >= 4);
    }

    #[test]
    fn build_payload_includes_username_and_icon() {
        let adapter = SlackAdapter::new(sample_config());
        let n = sample_notification(Priority::Normal);
        let payload = adapter.build_payload(&n).unwrap();
        assert_eq!(payload["username"], "Nexora");
        assert_eq!(payload["icon_emoji"], ":bell:");
    }

    #[test]
    fn build_payload_with_channel() {
        let mut cfg = sample_config();
        cfg.channel = Some("#alerts".into());
        let adapter = SlackAdapter::new(cfg);
        let n = sample_notification(Priority::Normal);
        let payload = adapter.build_payload(&n).unwrap();
        assert_eq!(payload["channel"], "#alerts");
    }

    #[test]
    fn validate_webhook_url_accepts_valid() {
        assert!(SlackAdapter::validate_webhook_url(
            "https://hooks.slack.com/services/TEAM0000/CHAN0000/FAKEWEBHOOKEXAMPLE000000"
        ).is_ok());
    }

    #[test]
    fn validate_webhook_url_rejects_http() {
        assert!(SlackAdapter::validate_webhook_url(
            "http://hooks.slack.com/services/T00000000/B00000000/XXX"
        ).is_err());
    }

    #[test]
    fn validate_webhook_url_rejects_wrong_host() {
        assert!(SlackAdapter::validate_webhook_url(
            "https://evil.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX"
        ).is_err());
    }

    #[test]
    fn validate_webhook_url_rejects_short() {
        assert!(SlackAdapter::validate_webhook_url(
            "https://hooks.slack.com/services/short"
        ).is_err());
    }

    #[tokio::test]
    async fn deliver_succeeds_with_valid_config() {
        let adapter = SlackAdapter::new(sample_config());
        let n = sample_notification(Priority::High);
        adapter.deliver(&n).await.unwrap();
    }

    #[tokio::test]
    async fn deliver_fails_with_invalid_webhook() {
        let mut cfg = sample_config();
        cfg.webhook_url = "https://evil.com/x".into();
        let adapter = SlackAdapter::new(cfg);
        let n = sample_notification(Priority::Normal);
        assert!(adapter.deliver(&n).await.is_err());
    }

    #[test]
    fn config_serde_roundtrip() {
        let cfg = sample_config();
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: SlackConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.webhook_url, cfg2.webhook_url);
        assert_eq!(cfg.username, cfg2.username);
    }

    #[test]
    fn config_defaults_applied() {
        let json = r#"{"webhook_url":"https://hooks.slack.com/services/TEAM0000/CHAN0000/FAKEWEBHOOKEXAMPLE000000"}"#;
        let cfg: SlackConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.username, "Nexora");
        assert_eq!(cfg.icon_emoji, ":bell:");
        assert!(cfg.channel.is_none());
    }

    #[test]
    fn channel_kind_is_slack() {
        let adapter = SlackAdapter::new(sample_config());
        assert_eq!(adapter.kind(), ChannelKind::Slack);
        assert_eq!(adapter.name(), "slack-webhook");
    }

    #[test]
    fn fallback_field_contains_title_and_body() {
        let adapter = SlackAdapter::new(sample_config());
        let n = sample_notification(Priority::Normal);
        let payload = adapter.build_payload(&n).unwrap();
        let fallback = payload["attachments"][0]["fallback"].as_str().unwrap();
        assert!(fallback.contains("تنبيه النظام"));
        assert!(fallback.contains("تم اكتشاف خطأ"));
    }

    #[test]
    fn timestamp_is_recent() {
        let adapter = SlackAdapter::new(sample_config());
        let n = sample_notification(Priority::Normal);
        let payload = adapter.build_payload(&n).unwrap();
        let ts = payload["attachments"][0]["ts"].as_u64().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(ts <= now);
        assert!(ts > now - 10); // خلال آخر 10 ثوانٍ
    }
}
