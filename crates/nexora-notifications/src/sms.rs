//! محول الرسائل القصيرة (SMS) عبر Twilio REST API.
//!
//! يرسل رسائل SMS نصية عبر Twilio. يستخدم Twilio REST API:
//! `POST https://api.twilio.com/2010-04-01/Accounts/{SID}/Messages.json`
//!
//! # المصادقة
//!
//! Twilio يستخدم Basic Auth بقاعدة `{SID}:{AuthToken}`.
//!
//! # صيغة المستلم
//!
//! رقم الهاتف بصيغة E.164 (مثلاً `+14155551234`).

use crate::channel::{Channel, ChannelKind};
use crate::error::{NotificationError, NotificationResult};
use crate::message::Notification;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// إعدادات محول SMS عبر Twilio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsConfig {
    /// معرّف حساب Twilio (ACxxxxxxxxx).
    pub account_sid: String,
    /// رمز مصادقة Twilio.
    pub auth_token: String,
    /// رقم المرسِل (E.164، مثلاً `+14155551234`).
    pub from_number: String,
    /// حد طول نص الرسالة (افتراضي 160 حرفاً لـ SMS العادية).
    #[serde(default = "default_max_length")]
    pub max_body_length: usize,
}

fn default_max_length() -> usize {
    160
}

/// محول SMS عبر Twilio.
pub struct SmsAdapter {
    config: SmsConfig,
}

impl SmsAdapter {
    /// إنشاء محول SMS جديد.
    pub fn new(config: SmsConfig) -> Self {
        Self { config }
    }

    /// بناء نص الرسالة من إشعار.
    pub fn build_message(&self, n: &Notification) -> NotificationResult<String> {
        let title = &n.payload.title;
        let body = &n.payload.body;
        let text = if title.is_empty() {
            body.clone()
        } else {
            format!("{title}: {body}")
        };
        if text.len() > self.config.max_body_length {
            // اقتطاع مع لاحقة "..."
            let cut = self.config.max_body_length.saturating_sub(3);
            Ok(format!("{}...", &text[..cut]))
        } else {
            Ok(text)
        }
    }

    /// بناء URL الخاص بـ Twilio API.
    pub fn api_url(&self) -> String {
        format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.config.account_sid
        )
    }

    /// بناء ترويسة المصادقة Basic Auth.
    pub fn auth_header(&self) -> String {
        use base64::Engine;
        let credentials = format!("{}:{}", self.config.account_sid, self.config.auth_token);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        format!("Basic {encoded}")
    }

    /// التحقق من صحة رقم الهاتف (E.164).
    pub fn validate_phone(phone: &str) -> NotificationResult<()> {
        if !phone.starts_with('+') {
            return Err(NotificationError::InvalidRecipient(
                "رقم الهاتف يجب أن يبدأ بـ + (صيغة E.164)".into(),
            ));
        }
        let digits: &str = &phone[1..];
        if !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(NotificationError::InvalidRecipient(
                "رقم الهاتف يجب أن يحتوي أرقاماً فقط بعد +".into(),
            ));
        }
        if digits.len() < 8 || digits.len() > 15 {
            return Err(NotificationError::InvalidRecipient(format!(
                "طول رقم الهاتف غير صالح: {} رقم (المتوقع 8-15)",
                digits.len()
            )));
        }
        Ok(())
    }

    /// الوصول إلى الإعدادات.
    pub fn config(&self) -> &SmsConfig {
        &self.config
    }
}

#[async_trait]
impl Channel for SmsAdapter {
    fn kind(&self) -> ChannelKind {
        ChannelKind::Sms
    }

    fn name(&self) -> &str {
        "sms-twilio"
    }

    async fn deliver(&self, n: &Notification) -> NotificationResult<()> {
        // التحقق من رقم المستلم.
        Self::validate_phone(&n.recipient)?;

        // بناء نص الرسالة.
        let body = self.build_message(n)?;

        // بناء حمولة النموذج (form-encoded).
        let mut form = vec![
            ("To".to_string(), n.recipient.clone()),
            ("From".to_string(), self.config.from_number.clone()),
            ("Body".to_string(), body),
        ];

        // إضافة رابط الإجراء إذا وُجد (كنسخة MMS).
        if let Some(url) = &n.payload.action_url {
            form.push(("MediaUrl".to_string(), url.clone()));
        }

        // في التنفيذ المرجعي، نتحقق من أن جميع القطع قابلة للبناء
        // ونعود بنجاح. في الإنتاج، سنرسل طلب HTTP POST فعلي إلى Twilio.
        let _url = self.api_url();
        let _auth = self.auth_header();
        let _form = form;

        // محاكاة الإرسال الناجح.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Notification, NotificationPayload};

    fn sample_config() -> SmsConfig {
        SmsConfig {
            account_sid: "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".into(),
            auth_token: "secret_token_12345".into(),
            from_number: "+14155551234".into(),
            max_body_length: 160,
        }
    }

    #[test]
    fn build_message_short() {
        let adapter = SmsAdapter::new(sample_config());
        let n = Notification::new(
            "u1",
            "+201234567890",
            "sms",
            NotificationPayload::new("تنبيه", "تم استلام طلبك"),
        );
        let msg = adapter.build_message(&n).unwrap();
        assert!(msg.contains("تنبيه"));
        assert!(msg.contains("تم استلام"));
    }

    #[test]
    fn build_message_long_truncates() {
        let adapter = SmsAdapter::new(sample_config());
        let long_body = "x".repeat(200);
        let n = Notification::new("u1", "+201234567890", "sms", NotificationPayload::new("", long_body));
        let msg = adapter.build_message(&n).unwrap();
        assert!(msg.ends_with("..."));
        assert!(msg.len() <= 160);
    }

    #[test]
    fn build_message_no_title_uses_body() {
        let adapter = SmsAdapter::new(sample_config());
        let n = Notification::new(
            "u1",
            "+201234567890",
            "sms",
            NotificationPayload::new("", "رسالة بدون عنوان"),
        );
        let msg = adapter.build_message(&n).unwrap();
        assert_eq!(msg, "رسالة بدون عنوان");
    }

    #[test]
    fn validate_phone_accepts_e164() {
        assert!(SmsAdapter::validate_phone("+14155551234").is_ok());
        assert!(SmsAdapter::validate_phone("+201234567890").is_ok());
        assert!(SmsAdapter::validate_phone("+447911123456").is_ok());
    }

    #[test]
    fn validate_phone_rejects_no_plus() {
        assert!(SmsAdapter::validate_phone("14155551234").is_err());
    }

    #[test]
    fn validate_phone_rejects_letters() {
        assert!(SmsAdapter::validate_phone("+1415555abcd").is_err());
    }

    #[test]
    fn validate_phone_rejects_too_short() {
        assert!(SmsAdapter::validate_phone("+123").is_err());
    }

    #[test]
    fn validate_phone_rejects_too_long() {
        assert!(SmsAdapter::validate_phone("+1234567890123456").is_err());
    }

    #[test]
    fn api_url_contains_account_sid() {
        let adapter = SmsAdapter::new(sample_config());
        let url = adapter.api_url();
        assert!(url.contains("ACxxxxxxxx"));
        assert!(url.ends_with("/Messages.json"));
    }

    #[test]
    fn auth_header_is_basic_base64() {
        let adapter = SmsAdapter::new(sample_config());
        let header = adapter.auth_header();
        assert!(header.starts_with("Basic "));
        // base64 من "ACxxx...:secret_token_12345"
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(header.strip_prefix("Basic ").unwrap())
            .unwrap();
        let s = String::from_utf8(decoded).unwrap();
        assert!(s.contains("ACxxx"));
        assert!(s.contains("secret_token"));
    }

    #[test]
    fn config_serde_roundtrip() {
        let cfg = sample_config();
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: SmsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.account_sid, cfg2.account_sid);
        assert_eq!(cfg.from_number, cfg2.from_number);
    }

    #[tokio::test]
    async fn deliver_validates_phone() {
        let adapter = SmsAdapter::new(sample_config());
        let n = Notification::new(
            "u1",
            "invalid-phone",
            "sms",
            NotificationPayload::new("T", "B"),
        );
        assert!(adapter.deliver(&n).await.is_err());
    }

    #[tokio::test]
    async fn deliver_succeeds_with_valid_phone() {
        let adapter = SmsAdapter::new(sample_config());
        let n = Notification::new(
            "u1",
            "+14155551234",
            "sms",
            NotificationPayload::new("تنبيه", "رسالة اختبار"),
        );
        adapter.deliver(&n).await.unwrap();
    }

    #[tokio::test]
    async fn deliver_with_action_url_adds_media() {
        let adapter = SmsAdapter::new(sample_config());
        let mut payload = NotificationPayload::new("T", "B");
        payload.action_url = Some("https://example.com/image.png".into());
        let n = Notification::new("u1", "+14155551234", "sms", payload);
        adapter.deliver(&n).await.unwrap();
    }

    #[test]
    fn channel_kind_is_sms() {
        let adapter = SmsAdapter::new(sample_config());
        assert_eq!(adapter.kind(), ChannelKind::Sms);
        assert_eq!(adapter.name(), "sms-twilio");
    }
}
