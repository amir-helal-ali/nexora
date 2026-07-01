//! Notification handler — dispatches notification commands.

use crate::types::{Notification, NotificationSeverity};
use crate::NotificationService;
use nxp_core::NxpError;
use nxp_core::error::protocol_codes;
use serde_json::Value;
use std::sync::Arc;

/// The Notification handler.
#[derive(Clone)]
pub struct NotificationHandler {
    service: Arc<NotificationService>,
}

impl std::fmt::Debug for NotificationHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationHandler")
            .field("service", &self.service)
            .finish()
    }
}

impl NotificationHandler {
    /// Construct a new handler.
    pub fn new(service: Arc<NotificationService>) -> Self {
        Self { service }
    }

    /// Execute a notification command.
    pub async fn execute(&self, command: &str, args: &Value) -> Result<Value, NxpError> {
        match command {
            "notification.create" => self.cmd_create(args),
            "notification.list" => self.cmd_list(args),
            "notification.unread_count" => self.cmd_unread_count(args),
            "notification.mark_read" => self.cmd_mark_read(args),
            "notification.mark_all_read" => self.cmd_mark_all_read(args),
            "notification.delete" => self.cmd_delete(args),
            "notification.delete_read" => self.cmd_delete_read(args),
            "notification.stats" => self.cmd_stats(),
            _ => Err(NxpError::protocol(
                protocol_codes::UNKNOWN_OPCODE,
                format!("unknown notification command: {}", command),
            )),
        }
    }

    fn cmd_create(&self, args: &Value) -> Result<Value, NxpError> {
        let user_id = args.get("user_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing user_id"))?;
        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");
        let severity_str = args.get("severity").and_then(|v| v.as_str()).unwrap_or("info");
        let severity = match severity_str {
            "success" => NotificationSeverity::Success,
            "warning" => NotificationSeverity::Warning,
            "error" => NotificationSeverity::Error,
            _ => NotificationSeverity::Info,
        };
        let mut notif = Notification::new(user_id, title, body, severity);
        if let Some(link) = args.get("link").and_then(|v| v.as_str()) {
            notif = notif.with_link(link);
        }
        if let Some(icon) = args.get("icon").and_then(|v| v.as_str()) {
            notif = notif.with_icon(icon);
        }
        let notif = self.service.store.create(notif);
        Ok(serde_json::json!({ "ok": true, "notification": notif }))
    }

    fn cmd_list(&self, args: &Value) -> Result<Value, NxpError> {
        let user_id = args.get("user_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing user_id"))?;
        let list = self.service.store.list_for_user(user_id);
        Ok(serde_json::json!({ "ok": true, "count": list.len(), "notifications": list }))
    }

    fn cmd_unread_count(&self, args: &Value) -> Result<Value, NxpError> {
        let user_id = args.get("user_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing user_id"))?;
        let count = self.service.store.unread_count(user_id);
        Ok(serde_json::json!({ "ok": true, "count": count }))
    }

    fn cmd_mark_read(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let notif = self.service.store.mark_read(id)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true, "notification": notif }))
    }

    fn cmd_mark_all_read(&self, args: &Value) -> Result<Value, NxpError> {
        let user_id = args.get("user_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing user_id"))?;
        let count = self.service.store.mark_all_read(user_id);
        Ok(serde_json::json!({ "ok": true, "marked_read": count }))
    }

    fn cmd_delete(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        self.service.store.delete(id)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true }))
    }

    fn cmd_delete_read(&self, args: &Value) -> Result<Value, NxpError> {
        let user_id = args.get("user_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing user_id"))?;
        let count = self.service.store.delete_read(user_id);
        Ok(serde_json::json!({ "ok": true, "deleted": count }))
    }

    fn cmd_stats(&self) -> Result<Value, NxpError> {
        Ok(serde_json::json!({
            "ok": true,
            "stats": {
                "total": self.service.store.count(),
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_core::NexoraCore;

    fn setup() -> NotificationHandler {
        let core = Arc::new(NexoraCore::new());
        let svc = Arc::new(NotificationService::new(core));
        NotificationHandler::new(svc)
    }

    #[tokio::test]
    async fn create_and_list() {
        let h = setup();
        h.execute("notification.create", &serde_json::json!({
            "user_id": "u1", "title": "Welcome", "body": "Hello!", "severity": "info"
        })).await.unwrap();
        let resp = h.execute("notification.list", &serde_json::json!({"user_id":"u1"})).await.unwrap();
        assert_eq!(resp["count"], 1);
    }

    #[tokio::test]
    async fn unread_count_works() {
        let h = setup();
        h.execute("notification.create", &serde_json::json!({"user_id":"u1","title":"A","body":"a"})).await.unwrap();
        h.execute("notification.create", &serde_json::json!({"user_id":"u1","title":"B","body":"b"})).await.unwrap();
        let resp = h.execute("notification.unread_count", &serde_json::json!({"user_id":"u1"})).await.unwrap();
        assert_eq!(resp["count"], 2);
    }

    #[tokio::test]
    async fn mark_read_works() {
        let h = setup();
        let resp = h.execute("notification.create", &serde_json::json!({"user_id":"u1","title":"A","body":"a"})).await.unwrap();
        let id = resp["notification"]["id"].as_str().unwrap();
        h.execute("notification.mark_read", &serde_json::json!({"id": id})).await.unwrap();
        let resp = h.execute("notification.unread_count", &serde_json::json!({"user_id":"u1"})).await.unwrap();
        assert_eq!(resp["count"], 0);
    }

    #[tokio::test]
    async fn unknown_command_rejected() {
        let h = setup();
        let err = h.execute("notification.nope", &serde_json::json!({})).await.unwrap_err();
        assert_eq!(err.code, protocol_codes::UNKNOWN_OPCODE);
    }
}
