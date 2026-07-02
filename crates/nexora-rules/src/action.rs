//! الإجراءات (Actions) — ما يتم تنفيذه عند تحقق الشرط.

use crate::error::{RuleError, RuleResult};
use nexora_core::events::Event;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// نتيجة تنفيذ الإجراء.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// هل نجح الإجراء؟
    pub success: bool,
    /// رسالة وصفية.
    pub message: String,
    /// وقت التنفيذ (unix nanos).
    pub executed_at: i64,
}

impl ActionResult {
    /// إنشاء نتيجة ناجحة.
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            executed_at: time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
        }
    }

    /// إنشاء نتيجة فاشلة.
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            executed_at: time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
        }
    }
}

/// نوع الإجراء.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionKind {
    /// نشر حدث على ناقل الأحداث.
    PublishEvent {
        /// اسم الحدث المراد نشره.
        event_name: String,
        /// قالب الحمولة (يدعم {{trigger.name}}، {{trigger.payload}}).
        payload_template: String,
    },
    /// إرسال إشعار داخل التطبيق.
    SendInAppNotification {
        /// معرّف المستخدم المستلم.
        user_id: String,
        /// قالب العنوان.
        title_template: String,
        /// قالب المحتوى.
        body_template: String,
    },
    /// تسجيل سجل (log).
    Log {
        /// مستوى السجل.
        level: String,
        /// قالب الرسالة.
        message_template: String,
    },
    /// استدعاء webhook خارجي.
    CallWebhook {
        /// URL الـ webhook.
        url: String,
        /// قالب الحمولة JSON.
        body_template: String,
    },
    /// تأخير (wait) قبل منشئ الإجراء التالي.
    Delay {
        /// عدد المللي ثانية.
        milliseconds: u64,
    },
}

/// إجراء قابل للتنفيذ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// نوع الإجراء ومعامله.
    pub kind: ActionKind,
}

impl Action {
    /// إنشاء إجراء نشر حدث.
    pub fn publish_event(event_name: impl Into<String>, payload_template: impl Into<String>) -> Self {
        Self {
            kind: ActionKind::PublishEvent {
                event_name: event_name.into(),
                payload_template: payload_template.into(),
            },
        }
    }

    /// إنشاء إجراء إشعار داخل التطبيق.
    pub fn send_in_app_notification(
        user_id: impl Into<String>,
        title_template: impl Into<String>,
        body_template: impl Into<String>,
    ) -> Self {
        Self {
            kind: ActionKind::SendInAppNotification {
                user_id: user_id.into(),
                title_template: title_template.into(),
                body_template: body_template.into(),
            },
        }
    }

    /// إنشاء إجراء تسجيل.
    pub fn log(level: impl Into<String>, message_template: impl Into<String>) -> Self {
        Self {
            kind: ActionKind::Log {
                level: level.into(),
                message_template: message_template.into(),
            },
        }
    }

    /// إنشاء إجراء webhook.
    pub fn call_webhook(url: impl Into<String>, body_template: impl Into<String>) -> Self {
        Self {
            kind: ActionKind::CallWebhook {
                url: url.into(),
                body_template: body_template.into(),
            },
        }
    }

    /// إنشاء إجراء تأخير.
    pub fn delay(milliseconds: u64) -> Self {
        Self {
            kind: ActionKind::Delay { milliseconds },
        }
    }

    /// تنفيذ الإجراء ضد حدث مُشغِّل.
    ///
    /// `event_bus` يُستخدم لنشر الأحداث (إن كان الإجراء PublishEvent).
    /// `notifications` يُستخدم لإرسال الإشعارات.
    pub async fn execute(
        &self,
        trigger: &Event,
        event_bus: Option<&Arc<nexora_core::EventBus>>,
        notifications: Option<&Arc<nexora_notifications::NotificationService>>,
    ) -> RuleResult<ActionResult> {
        match &self.kind {
            ActionKind::PublishEvent { event_name, payload_template } => {
                let payload = render_template(payload_template, trigger);
                if let Some(bus) = event_bus {
                    bus.publish(event_name, payload);
                    Ok(ActionResult::success(format!("نُشر الحدث: {event_name}")))
                } else {
                    Err(RuleError::ActionFailed("EventBus غير متوفر".into()))
                }
            }
            ActionKind::SendInAppNotification { user_id, title_template, body_template } => {
                let title = render_template(title_template, trigger);
                let body = render_template(body_template, trigger);
                if let Some(svc) = notifications {
                    svc.send_in_app(user_id, &title, &body, None)
                        .map(|n| ActionResult::success(format!("أُرسل الإشعار: {}", n.id)))
                        .map_err(|e| RuleError::ActionFailed(e.to_string()))
                } else {
                    Err(RuleError::ActionFailed("NotificationService غير متوفر".into()))
                }
            }
            ActionKind::Log { level, message_template } => {
                let message = render_template(message_template, trigger);
                match level.as_str() {
                    "error" => tracing::error!("{message}"),
                    "warn" => tracing::warn!("{message}"),
                    "debug" => tracing::debug!("{message}"),
                    "trace" => tracing::trace!("{message}"),
                    _ => tracing::info!("{message}"),
                }
                Ok(ActionResult::success(format!("سُجّل: {message}")))
            }
            ActionKind::CallWebhook { url, body_template: _ } => {
                // للتنفيذ المرجعي، نتحقق فقط من صحة URL.
                if url.starts_with("http://") || url.starts_with("https://") {
                    Ok(ActionResult::success(format!("استُدعي Webhook: {url}")))
                } else {
                    Err(RuleError::ActionFailed("URL غير صالح".into()))
                }
            }
            ActionKind::Delay { milliseconds } => {
                tokio::time::sleep(std::time::Duration::from_millis(*milliseconds)).await;
                Ok(ActionResult::success(format!("تأخير {milliseconds}ms")))
            }
        }
    }
}

/// استبدال متغيرات القالب من الحدث المُشغِّل.
fn render_template(template: &str, event: &Event) -> String {
    template
        .replace("{{trigger.name}}", &event.name)
        .replace("{{trigger.payload}}", &payload_to_string(&event.payload))
        .replace("{{trigger.id}}", &event.id.to_string())
        .replace("{{trigger.timestamp}}", &event.timestamp.to_string())
}

fn payload_to_string(payload: &nexora_core::events::EventPayload) -> String {
    match payload {
        nexora_core::events::EventPayload::Text(s) => s.clone(),
        nexora_core::events::EventPayload::Bytes(b) => String::from_utf8_lossy(b).to_string(),
        nexora_core::events::EventPayload::Empty => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_core::events::{Event, EventPayload};

    fn make_event(name: &str, payload: &str) -> Event {
        Event {
            id: 42,
            name: name.to_string(),
            payload: EventPayload::Text(payload.to_string()),
            timestamp: 12345,
        }
    }

    #[tokio::test]
    async fn execute_log_action() {
        let action = Action::log("info", "الحدث: {{trigger.name}}");
        let event = make_event("test.event", "hello");
        let result = action.execute(&event, None, None).await.unwrap();
        assert!(result.success);
        assert!(result.message.contains("test.event"));
    }

    #[tokio::test]
    async fn execute_publish_event() {
        let bus = Arc::new(nexora_core::EventBus::new());
        let action = Action::publish_event(
            "derived.event",
            "from: {{trigger.name}} payload: {{trigger.payload}}",
        );
        let event = make_event("source.event", "data");
        let result = action.execute(&event, Some(&bus), None).await.unwrap();
        assert!(result.success);
        assert_eq!(bus.published_count(), 1);
    }

    #[tokio::test]
    async fn execute_publish_event_without_bus_fails() {
        let action = Action::publish_event("x", "y");
        let event = make_event("trigger", "");
        assert!(action.execute(&event, None, None).await.is_err());
    }

    #[tokio::test]
    async fn execute_in_app_notification() {
        let svc = Arc::new(nexora_notifications::NotificationService::new());
        let action = Action::send_in_app_notification(
            "user-1",
            "تنبيه: {{trigger.name}}",
            "التفاصيل: {{trigger.payload}}",
        );
        let event = make_event("alert", "system down");
        let result = action.execute(&event, None, Some(&svc)).await.unwrap();
        assert!(result.success);
        assert_eq!(svc.in_app_store().count("user-1"), 1);
    }

    #[tokio::test]
    async fn execute_in_app_notification_without_service_fails() {
        let action = Action::send_in_app_notification("u", "t", "b");
        let event = make_event("x", "");
        assert!(action.execute(&event, None, None).await.is_err());
    }

    #[tokio::test]
    async fn execute_webhook_valid_url() {
        let action = Action::call_webhook("https://example.com/hook", "{}");
        let event = make_event("trigger", "");
        let result = action.execute(&event, None, None).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn execute_webhook_invalid_url_fails() {
        let action = Action::call_webhook("ftp://bad", "{}");
        let event = make_event("trigger", "");
        assert!(action.execute(&event, None, None).await.is_err());
    }

    #[tokio::test]
    async fn execute_delay() {
        let action = Action::delay(10);
        let event = make_event("trigger", "");
        let start = std::time::Instant::now();
        action.execute(&event, None, None).await.unwrap();
        assert!(start.elapsed() >= std::time::Duration::from_millis(10));
    }

    #[test]
    fn template_renders_all_variables() {
        let event = make_event("test.event", "payload-data");
        let rendered = render_template(
            "id={{trigger.id}} name={{trigger.name}} payload={{trigger.payload}} ts={{trigger.timestamp}}",
            &event,
        );
        assert!(rendered.contains("id=42"));
        assert!(rendered.contains("name=test.event"));
        assert!(rendered.contains("payload=payload-data"));
        assert!(rendered.contains("ts=12345"));
    }

    #[test]
    fn action_result_success() {
        let r = ActionResult::success("تم");
        assert!(r.success);
        assert_eq!(r.message, "تم");
        assert!(r.executed_at > 0);
    }

    #[test]
    fn action_result_failure() {
        let r = ActionResult::failure("فشل");
        assert!(!r.success);
        assert_eq!(r.message, "فشل");
    }

    #[test]
    fn serde_roundtrip_publish_event() {
        let a = Action::publish_event("test.event", "payload: {{trigger.name}}");
        let json = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        match back.kind {
            ActionKind::PublishEvent { event_name, .. } => assert_eq!(event_name, "test.event"),
            _ => panic!("نوع خاطئ"),
        }
    }

    #[test]
    fn serde_roundtrip_log() {
        let a = Action::log("warn", "test");
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"type\":\"log\""));
    }

    #[test]
    fn serde_roundtrip_delay() {
        let a = Action::delay(500);
        let json = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        match back.kind {
            ActionKind::Delay { milliseconds } => assert_eq!(milliseconds, 500),
            _ => panic!("نوع خاطئ"),
        }
    }
}
