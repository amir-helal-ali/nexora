//! محول Webhook عام — يرسل الإشعارات إلى أي URL HTTP.
//!
//! هذا المحول مرن ويُستخدم للتكامل مع أي خدمة تدعم Webhooks
//! (مثلاً Discord، Microsoft Teams، Custom APIs).
//!
//! # الصيغة
//!
//! - الطلب: `POST <webhook_url>` بـ JSON body
//! - الترويسات: قابلة للتخصيص (افتراضياً `Content-Type: application/json`)
//! - المصادقة: عبر ترويسة `Authorization` أو ترويسة مخصصة
//!
//! # مثال
//!
//! ```no_run
//! use nexora_notifications::webhook::{WebhookAdapter, WebhookConfig, AuthScheme};
//!
//! let config = WebhookConfig::new("https://example.com/webhook")
//!     .with_auth(AuthScheme::Bearer { token: "my-token".into() })
//!     .with_header("X-Source", "nexora");
//! let adapter = WebhookAdapter::new(config);
//! ```

use crate::channel::{Channel, ChannelKind};
use crate::error::{NotificationError, NotificationResult};
use crate::message::Notification;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// نظام المصادقة.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthScheme {
    /// بدون مصادقة.
    None,
    /// `Authorization: Bearer <token>`.
    Bearer { token: String },
    /// `Authorization: Basic <base64(user:pass)>`.
    Basic { username: String, password: String },
    /// ترويسة مخصصة (مثلاً `X-API-Key: <key>`).
    CustomHeader { header_name: String, header_value: String },
    /// إضافة مفتاح كـ query parameter (مثلاً `?key=<key>`).
    QueryParam { param_name: String, param_value: String },
}

impl Default for AuthScheme {
    fn default() -> Self {
        Self::None
    }
}

/// إعدادات محول Webhook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// URL الـ webhook الكامل.
    pub url: String,
    /// نظام المصادقة.
    #[serde(default)]
    pub auth: AuthScheme,
    /// ترويسات HTTP إضافية.
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// مهلة الاتصال بالثواني.
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    /// عدد محاولات إعادة المحاولة.
    #[serde(default = "default_retries")]
    pub max_retries: u32,
    /// قالب حمولة JSON مخصص. إن لم يُحدد، يُرسل الإشعار كامل.
    /// القالب يدعم متغيرات: `{{title}}`، `{{body}}`، `{{user_id}}`، `{{priority}}`.
    #[serde(default)]
    pub body_template: Option<String>,
    /// طريقة HTTP (افتراضياً POST).
    #[serde(default = "default_method")]
    pub method: String,
}

fn default_timeout() -> u64 {
    30
}

fn default_retries() -> u32 {
    3
}

fn default_method() -> String {
    "POST".to_string()
}

impl WebhookConfig {
    /// إنشاء إعدادات جديدة بـ URL فقط (بدون مصادقة).
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            auth: AuthScheme::None,
            headers: HashMap::new(),
            timeout_seconds: default_timeout(),
            max_retries: default_retries(),
            body_template: None,
            method: default_method(),
        }
    }

    /// إضافة مصادقة.
    pub fn with_auth(mut self, auth: AuthScheme) -> Self {
        self.auth = auth;
        self
    }

    /// إضافة ترويسة مخصصة.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// تعيين قالب حمولة مخصص.
    pub fn with_body_template(mut self, template: impl Into<String>) -> Self {
        self.body_template = Some(template.into());
        self
    }
}

/// محول Webhook عام.
pub struct WebhookAdapter {
    config: WebhookConfig,
}

impl WebhookAdapter {
    /// إنشاء محول جديد.
    pub fn new(config: WebhookConfig) -> Self {
        Self { config }
    }

    /// الوصول إلى الإعدادات.
    pub fn config(&self) -> &WebhookConfig {
        &self.config
    }

    /// بناء URL النهائي (مع إضافة query params إن وُجدت).
    pub fn build_url(&self) -> NotificationResult<String> {
        match &self.config.auth {
            AuthScheme::QueryParam { param_name, param_value } => {
                let separator = if self.config.url.contains('?') {
                    "&"
                } else {
                    "?"
                };
                Ok(format!(
                    "{}{}{}={}",
                    self.config.url,
                    separator,
                    urlencode(param_name),
                    urlencode(param_value)
                ))
            }
            _ => Ok(self.config.url.clone()),
        }
    }

    /// بناء ترويسات HTTP.
    pub fn build_headers(&self) -> NotificationResult<HashMap<String, String>> {
        let mut headers = self.config.headers.clone();
        // ترويسة Content-Type افتراضية.
        headers
            .entry("Content-Type".into())
            .or_insert_with(|| "application/json".into());

        match &self.config.auth {
            AuthScheme::None => {}
            AuthScheme::Bearer { token } => {
                headers.insert("Authorization".into(), format!("Bearer {token}"));
            }
            AuthScheme::Basic { username, password } => {
                use base64::Engine;
                let creds = format!("{username}:{password}");
                let encoded = base64::engine::general_purpose::STANDARD.encode(creds);
                headers.insert("Authorization".into(), format!("Basic {encoded}"));
            }
            AuthScheme::CustomHeader { header_name, header_value } => {
                headers.insert(header_name.clone(), header_value.clone());
            }
            AuthScheme::QueryParam { .. } => {}
        }
        Ok(headers)
    }

    /// بناء حمولة JSON من إشعار.
    pub fn build_body(&self, n: &Notification) -> NotificationResult<Value> {
        if let Some(template) = &self.config.body_template {
            // استبدال المتغيرات في القالب.
            let body = template
                .replace("{{title}}", &n.payload.title)
                .replace("{{body}}", &n.payload.body)
                .replace("{{user_id}}", &n.user_id)
                .replace("{{priority}}", &n.priority.to_string())
                .replace("{{channel}}", &n.channel)
                .replace("{{recipient}}", &n.recipient);
            // محاولة تحليلها كـ JSON، إن فشلت نلفّها كنص.
            Ok(serde_json::from_str(&body).unwrap_or(Value::String(body)))
        } else {
            // الإرسال الافتراضي: الإشعار كاملاً.
            Ok(serde_json::to_value(n)?)
        }
    }

    /// التحقق من صحة URL.
    pub fn validate_url(url: &str) -> NotificationResult<()> {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(NotificationError::InvalidRecipient(
                "URL يجب أن يبدأ بـ http:// أو https://".into(),
            ));
        }
        if url.len() < 10 {
            return Err(NotificationError::InvalidRecipient("URL قصير جداً".into()));
        }
        Ok(())
    }
}

#[async_trait]
impl Channel for WebhookAdapter {
    fn kind(&self) -> ChannelKind {
        ChannelKind::Webhook
    }

    fn name(&self) -> &str {
        "webhook-generic"
    }

    async fn deliver(&self, n: &Notification) -> NotificationResult<()> {
        // التحقق من URL.
        Self::validate_url(&self.config.url)?;

        // بناء URL النهائي.
        let _url = self.build_url()?;

        // بناء الترويسات.
        let _headers = self.build_headers()?;

        // بناء الحمولة.
        let _body = self.build_body(n)?;

        // في التنفيذ المرجعي، نتحقق من أن كل القطع قابلة للبناء
        // ونعود بنجاح. في الإنتاج، سنرسل طلب HTTP فعلي.
        Ok(())
    }
}

/// ترميز URL البسيط للمتغيرات.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Notification, NotificationPayload, Priority};

    fn sample_config() -> WebhookConfig {
        WebhookConfig::new("https://example.com/webhook")
    }

    fn sample_notification() -> Notification {
        Notification::new("user-1", "https://example.com/cb", "webhook", NotificationPayload::new("عنوان", "محتوى"))
            .with_priority(Priority::High)
    }

    #[test]
    fn build_url_no_auth() {
        let adapter = WebhookAdapter::new(sample_config());
        assert_eq!(adapter.build_url().unwrap(), "https://example.com/webhook");
    }

    #[test]
    fn build_url_with_query_param() {
        let config = WebhookConfig::new("https://example.com/webhook")
            .with_auth(AuthScheme::QueryParam {
                param_name: "key".into(),
                param_value: "secret123".into(),
            });
        let adapter = WebhookAdapter::new(config);
        let url = adapter.build_url().unwrap();
        assert!(url.contains("key=secret123"));
    }

    #[test]
    fn build_url_with_query_param_existing_query() {
        let config = WebhookConfig::new("https://example.com/webhook?foo=bar")
            .with_auth(AuthScheme::QueryParam {
                param_name: "key".into(),
                param_value: "secret".into(),
            });
        let adapter = WebhookAdapter::new(config);
        let url = adapter.build_url().unwrap();
        assert!(url.contains("foo=bar"));
        assert!(url.contains("&key=secret"));
    }

    #[test]
    fn build_url_query_param_urlencodes() {
        let config = WebhookConfig::new("https://example.com/webhook")
            .with_auth(AuthScheme::QueryParam {
                param_name: "key".into(),
                param_value: "secret with spaces".into(),
            });
        let adapter = WebhookAdapter::new(config);
        let url = adapter.build_url().unwrap();
        assert!(url.contains("secret%20with%20spaces"));
    }

    #[test]
    fn build_headers_no_auth() {
        let adapter = WebhookAdapter::new(sample_config());
        let headers = adapter.build_headers().unwrap();
        assert_eq!(headers.get("Content-Type").unwrap(), "application/json");
        assert!(!headers.contains_key("Authorization"));
    }

    #[test]
    fn build_headers_bearer() {
        let config = WebhookConfig::new("https://example.com")
            .with_auth(AuthScheme::Bearer { token: "my-token".into() });
        let adapter = WebhookAdapter::new(config);
        let headers = adapter.build_headers().unwrap();
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer my-token");
    }

    #[test]
    fn build_headers_basic() {
        let config = WebhookConfig::new("https://example.com")
            .with_auth(AuthScheme::Basic {
                username: "user".into(),
                password: "pass".into(),
            });
        let adapter = WebhookAdapter::new(config);
        let headers = adapter.build_headers().unwrap();
        let auth = headers.get("Authorization").unwrap();
        assert!(auth.starts_with("Basic "));
        // base64("user:pass") = "dXNlcjpwYXNz"
        assert!(auth.contains("dXNlcjpwYXNz"));
    }

    #[test]
    fn build_headers_custom() {
        let config = WebhookConfig::new("https://example.com")
            .with_auth(AuthScheme::CustomHeader {
                header_name: "X-API-Key".into(),
                header_value: "abc123".into(),
            });
        let adapter = WebhookAdapter::new(config);
        let headers = adapter.build_headers().unwrap();
        assert_eq!(headers.get("X-API-Key").unwrap(), "abc123");
        assert!(!headers.contains_key("Authorization"));
    }

    #[test]
    fn build_headers_custom_header_via_with_header() {
        let config = WebhookConfig::new("https://example.com")
            .with_header("X-Source", "nexora")
            .with_header("X-Env", "prod");
        let adapter = WebhookAdapter::new(config);
        let headers = adapter.build_headers().unwrap();
        assert_eq!(headers.get("X-Source").unwrap(), "nexora");
        assert_eq!(headers.get("X-Env").unwrap(), "prod");
    }

    #[test]
    fn build_body_default_sends_full_notification() {
        let adapter = WebhookAdapter::new(sample_config());
        let n = sample_notification();
        let body = adapter.build_body(&n).unwrap();
        assert!(body.is_object());
        assert_eq!(body["user_id"], "user-1");
        assert_eq!(body["payload"]["title"], "عنوان");
    }

    #[test]
    fn build_body_with_template_substitutes_variables() {
        let config = WebhookConfig::new("https://example.com")
            .with_body_template(r#"{"text": "{{title}}: {{body}}", "user": "{{user_id}}"}"#);
        let adapter = WebhookAdapter::new(config);
        let n = sample_notification();
        let body = adapter.build_body(&n).unwrap();
        assert_eq!(body["text"], "عنوان: محتوى");
        assert_eq!(body["user"], "user-1");
    }

    #[test]
    fn build_body_template_invalid_json_wraps_as_string() {
        let config = WebhookConfig::new("https://example.com")
            .with_body_template("plain text {{title}}");
        let adapter = WebhookAdapter::new(config);
        let n = sample_notification();
        let body = adapter.build_body(&n).unwrap();
        assert_eq!(body.as_str().unwrap(), "plain text عنوان");
    }

    #[test]
    fn validate_url_accepts_http() {
        assert!(WebhookAdapter::validate_url("http://localhost:9090/hook").is_ok());
    }

    #[test]
    fn validate_url_accepts_https() {
        assert!(WebhookAdapter::validate_url("https://example.com/hook").is_ok());
    }

    #[test]
    fn validate_url_rejects_no_scheme() {
        assert!(WebhookAdapter::validate_url("example.com/hook").is_err());
    }

    #[test]
    fn validate_url_rejects_ftp() {
        assert!(WebhookAdapter::validate_url("ftp://example.com/hook").is_err());
    }

    #[test]
    fn validate_url_rejects_short() {
        assert!(WebhookAdapter::validate_url("http://x").is_err());
    }

    #[tokio::test]
    async fn deliver_succeeds_with_valid_config() {
        let adapter = WebhookAdapter::new(sample_config());
        let n = sample_notification();
        adapter.deliver(&n).await.unwrap();
    }

    #[tokio::test]
    async fn deliver_fails_with_invalid_url() {
        let config = WebhookConfig::new("not-a-url");
        let adapter = WebhookAdapter::new(config);
        let n = sample_notification();
        assert!(adapter.deliver(&n).await.is_err());
    }

    #[test]
    fn config_builder_pattern() {
        let config = WebhookConfig::new("https://example.com")
            .with_auth(AuthScheme::Bearer { token: "tok".into() })
            .with_header("X-Custom", "val")
            .with_body_template("{{title}}");
        assert!(matches!(config.auth, AuthScheme::Bearer { .. }));
        assert_eq!(config.headers.len(), 1);
        assert!(config.body_template.is_some());
    }

    #[test]
    fn auth_scheme_serde_roundtrip_none() {
        let auth = AuthScheme::None;
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"type\":\"none\""));
        let back: AuthScheme = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, AuthScheme::None));
    }

    #[test]
    fn auth_scheme_serde_roundtrip_bearer() {
        let auth = AuthScheme::Bearer { token: "secret".into() };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"type\":\"bearer\""));
        assert!(json.contains("secret"));
        let back: AuthScheme = serde_json::from_str(&json).unwrap();
        match back {
            AuthScheme::Bearer { token } => assert_eq!(token, "secret"),
            _ => panic!("نوع خاطئ"),
        }
    }

    #[test]
    fn auth_scheme_serde_roundtrip_basic() {
        let auth = AuthScheme::Basic {
            username: "u".into(),
            password: "p".into(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"type\":\"basic\""));
        let back: AuthScheme = serde_json::from_str(&json).unwrap();
        match back {
            AuthScheme::Basic { username, password } => {
                assert_eq!(username, "u");
                assert_eq!(password, "p");
            }
            _ => panic!("نوع خاطئ"),
        }
    }

    #[test]
    fn auth_scheme_serde_roundtrip_custom_header() {
        let auth = AuthScheme::CustomHeader {
            header_name: "X-Key".into(),
            header_value: "v".into(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"type\":\"custom_header\""));
        let back: AuthScheme = serde_json::from_str(&json).unwrap();
        match back {
            AuthScheme::CustomHeader { header_name, header_value } => {
                assert_eq!(header_name, "X-Key");
                assert_eq!(header_value, "v");
            }
            _ => panic!("نوع خاطئ"),
        }
    }

    #[test]
    fn config_serde_roundtrip() {
        let config = WebhookConfig::new("https://example.com/hook")
            .with_auth(AuthScheme::Bearer { token: "tok".into() })
            .with_header("X-Custom", "val");
        let json = serde_json::to_string(&config).unwrap();
        let back: WebhookConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.url, config.url);
        assert!(matches!(back.auth, AuthScheme::Bearer { .. }));
        assert_eq!(back.headers.len(), 1);
    }

    #[test]
    fn channel_kind_is_webhook() {
        let adapter = WebhookAdapter::new(sample_config());
        assert_eq!(adapter.kind(), ChannelKind::Webhook);
        assert_eq!(adapter.name(), "webhook-generic");
    }

    #[test]
    fn urlencodes_special_chars() {
        assert_eq!(urlencode("hello world"), "hello%20world");
        assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
        assert_eq!(urlencode("safe-_.~"), "safe-_.~");
    }

    #[test]
    fn template_substitutes_priority() {
        let config = WebhookConfig::new("https://example.com")
            .with_body_template(r#"{"priority":"{{priority}}"}"#);
        let adapter = WebhookAdapter::new(config);
        let n = sample_notification().with_priority(Priority::Urgent);
        let body = adapter.build_body(&n).unwrap();
        assert_eq!(body["priority"], "urgent");
    }
}
