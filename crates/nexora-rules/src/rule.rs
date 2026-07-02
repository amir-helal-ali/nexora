//! تعريف القاعدة (Rule) — تربط المُشغِّل + الشرط + الإجراء.

use crate::action::Action;
use crate::condition::Condition;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// معرّف القاعدة.
pub type RuleId = String;

/// حالة القاعدة.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleStatus {
    /// القاعدة مفعّلة وتُقيَّم عند كل حدث.
    Enabled,
    /// القاعدة معطّلة مؤقتاً.
    Disabled,
    /// القاعدة محذوفة (للحفظ الناعم).
    Deleted,
}

impl std::fmt::Display for RuleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Enabled => f.write_str("enabled"),
            Self::Disabled => f.write_str("disabled"),
            Self::Deleted => f.write_str("deleted"),
        }
    }
}

/// قاعدة أتمتة.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// معرّف فريد للقاعدة.
    pub id: RuleId,
    /// اسم العرض.
    pub name: String,
    /// وصف اختياري.
    pub description: String,
    /// الشرط الذي يحدد متى تُنفّذ القاعدة.
    pub condition: Condition,
    /// الإجراء الذي يُنفَّذ عند تحقق الشرط.
    pub actions: Vec<Action>,
    /// حالة القاعدة.
    pub status: RuleStatus,
    /// أولوية القاعدة (الأقل يُنفَّذ أولاً).
    pub priority: i32,
    /// وقت الإنشاء (unix nanos).
    pub created_at: i64,
    /// وقت آخر تعديل.
    pub updated_at: i64,
    /// عدد مرات التنفيذ.
    pub execution_count: u64,
    /// عدد مرات النجاح.
    pub success_count: u64,
    /// عدد مرات الفشل.
    pub failure_count: u64,
    /// وقت آخر تنفيذ (unix nanos)، إن وُجد.
    pub last_executed_at: Option<i64>,
}

impl Rule {
    /// إنشاء قاعدة جديدة بمعرّف تلقائي.
    pub fn new(
        name: impl Into<String>,
        condition: Condition,
        actions: Vec<Action>,
    ) -> Self {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            condition,
            actions,
            status: RuleStatus::Enabled,
            priority: 100,
            created_at: now,
            updated_at: now,
            execution_count: 0,
            success_count: 0,
            failure_count: 0,
            last_executed_at: None,
        }
    }

    /// تعيين الوصف.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// تعيين الأولوية.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// تفعيل القاعدة.
    pub fn enable(&mut self) {
        self.status = RuleStatus::Enabled;
        self.touch();
    }

    /// تعطيل القاعدة.
    pub fn disable(&mut self) {
        self.status = RuleStatus::Disabled;
        self.touch();
    }

    /// تحديث طابع `updated_at`.
    fn touch(&mut self) {
        self.updated_at = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
    }

    /// هل القاعدة مفعّلة؟
    pub fn is_enabled(&self) -> bool {
        self.status == RuleStatus::Enabled
    }

    /// تسجيل تنفيذ.
    pub fn record_execution(&mut self, success: bool) {
        self.execution_count += 1;
        if success {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }
        self.last_executed_at =
            Some(OffsetDateTime::now_utc().unix_timestamp_nanos() as i64);
    }

    /// نسبة النجاح.
    pub fn success_rate(&self) -> f64 {
        if self.execution_count == 0 {
            return 0.0;
        }
        self.success_count as f64 / self.execution_count as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::condition::Condition;

    #[test]
    fn new_rule_has_enabled_status() {
        let r = Rule::new(
            "test",
            Condition::always(),
            vec![Action::log("info", "test")],
        );
        assert_eq!(r.status, RuleStatus::Enabled);
        assert!(r.is_enabled());
        assert_eq!(r.execution_count, 0);
        assert_eq!(r.priority, 100);
        assert!(!r.id.is_empty());
    }

    #[test]
    fn enable_disable() {
        let mut r = Rule::new("test", Condition::always(), vec![]);
        r.disable();
        assert!(!r.is_enabled());
        assert_eq!(r.status, RuleStatus::Disabled);
        r.enable();
        assert!(r.is_enabled());
    }

    #[test]
    fn record_execution_success() {
        let mut r = Rule::new("test", Condition::always(), vec![]);
        r.record_execution(true);
        r.record_execution(true);
        r.record_execution(false);
        assert_eq!(r.execution_count, 3);
        assert_eq!(r.success_count, 2);
        assert_eq!(r.failure_count, 1);
        assert!(r.last_executed_at.is_some());
    }

    #[test]
    fn success_rate() {
        let mut r = Rule::new("test", Condition::always(), vec![]);
        assert_eq!(r.success_rate(), 0.0);
        r.record_execution(true);
        r.record_execution(true);
        r.record_execution(false);
        assert!((r.success_rate() - 2.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn with_description_and_priority() {
        let r = Rule::new("test", Condition::always(), vec![])
            .with_description("وصف")
            .with_priority(50);
        assert_eq!(r.description, "وصف");
        assert_eq!(r.priority, 50);
    }

    #[test]
    fn serde_roundtrip() {
        let r = Rule::new(
            "test",
            Condition::event_name_matches("billing.*"),
            vec![Action::log("info", "{{trigger.name}}")],
        )
        .with_description("test rule")
        .with_priority(10);
        let json = serde_json::to_string(&r).unwrap();
        let back: Rule = serde_json::from_str(&json).unwrap();
        assert_eq!(r.id, back.id);
        assert_eq!(r.name, back.name);
        assert_eq!(r.priority, back.priority);
    }

    #[test]
    fn status_display() {
        assert_eq!(RuleStatus::Enabled.to_string(), "enabled");
        assert_eq!(RuleStatus::Disabled.to_string(), "disabled");
        assert_eq!(RuleStatus::Deleted.to_string(), "deleted");
    }

    #[test]
    fn touch_updates_timestamp() {
        let mut r = Rule::new("test", Condition::always(), vec![]);
        let original = r.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(1));
        r.disable();
        assert!(r.updated_at > original);
    }
}
