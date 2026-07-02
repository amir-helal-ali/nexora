//! مدخل سجل التدقيق.

use crate::category::AuditCategory;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use time::OffsetDateTime;

/// معرّف مدخل التدقيق.
pub type AuditEntryId = String;

/// مدخل واحد في سجل التدقيق.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// معرّف فريد.
    pub id: AuditEntryId,
    /// من قام بالإجراء (user_id, service_id, أو "system").
    pub actor: String,
    /// الإجراء الذي تم (مثلاً "login"، "create_user"، "install_module").
    pub action: String,
    /// الهدف الذي أُجري عليه الإجراء (مثلاً "user-123"، "module-auth").
    pub target: String,
    /// فئة الإجراء.
    pub category: AuditCategory,
    /// هل نجح الإجراء؟
    pub success: bool,
    /// رسالة خطأ (إن فشل).
    pub error: Option<String>,
    /// بيانات إضافية (IP, user agent, إلخ).
    pub metadata: HashMap<String, String>,
    /// وقت الإجراء (unix nanos).
    pub timestamp: i64,
}

impl AuditEntry {
    /// إنشاء مدخل جديد بمعرّف تلقائي.
    pub fn new(actor: impl Into<String>, action: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            actor: actor.into(),
            action: action.into(),
            target: target.into(),
            category: AuditCategory::Other,
            success: true,
            error: None,
            metadata: HashMap::new(),
            timestamp: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
        }
    }

    /// تعيين الفئة.
    pub fn with_category(mut self, category: AuditCategory) -> Self {
        self.category = category;
        self
    }

    /// تعيين النجاح/الفشل.
    pub fn with_success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    /// تعيين رسالة الخطأ.
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self.success = false;
        self
    }

    /// إضافة بيانات وصفية (metadata).
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// تعيين الطابع الزمني (للاختبار).
    pub fn with_timestamp(mut self, ts: i64) -> Self {
        self.timestamp = ts;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_entry_defaults() {
        let e = AuditEntry::new("user-1", "login", "session-1");
        assert_eq!(e.actor, "user-1");
        assert_eq!(e.action, "login");
        assert_eq!(e.target, "session-1");
        assert_eq!(e.category, AuditCategory::Other);
        assert!(e.success);
        assert!(e.error.is_none());
        assert!(e.metadata.is_empty());
        assert!(e.timestamp > 0);
        assert!(!e.id.is_empty());
    }

    #[test]
    fn builder_methods() {
        let e = AuditEntry::new("u", "a", "t")
            .with_category(AuditCategory::Auth)
            .with_metadata("ip", "192.168.1.1")
            .with_metadata("ua", "curl/8");
        assert_eq!(e.category, AuditCategory::Auth);
        assert_eq!(e.metadata.len(), 2);
        assert_eq!(e.metadata.get("ip").unwrap(), "192.168.1.1");
    }

    #[test]
    fn with_error_sets_success_false() {
        let e = AuditEntry::new("u", "a", "t").with_error("failed");
        assert!(!e.success);
        assert_eq!(e.error, Some("failed".into()));
    }

    #[test]
    fn with_success_false() {
        let e = AuditEntry::new("u", "a", "t").with_success(false);
        assert!(!e.success);
        assert!(e.error.is_none());
    }

    #[test]
    fn with_timestamp() {
        let e = AuditEntry::new("u", "a", "t").with_timestamp(12345);
        assert_eq!(e.timestamp, 12345);
    }

    #[test]
    fn serde_roundtrip() {
        let e = AuditEntry::new("u", "a", "t")
            .with_category(AuditCategory::Billing)
            .with_metadata("amount", "100");
        let json = serde_json::to_string(&e).unwrap();
        let back: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(e.id, back.id);
        assert_eq!(e.actor, back.actor);
        assert_eq!(e.category, back.category);
        assert_eq!(back.metadata.get("amount").unwrap(), "100");
    }

    #[test]
    fn unique_ids() {
        let e1 = AuditEntry::new("u", "a", "t");
        let e2 = AuditEntry::new("u", "a", "t");
        assert_ne!(e1.id, e2.id);
    }
}
