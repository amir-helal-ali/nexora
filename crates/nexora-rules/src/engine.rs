//! محرك القواعد — يدير القواعد وينفّذها عند نشر الأحداث.

use crate::action::Action;
use crate::error::RuleResult;
use crate::rule::{Rule, RuleId, RuleStatus};
use nexora_core::events::{Event, EventSubscriber, EventBus};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// إحصائيات المحرك.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RuleEngineStats {
    /// إجمالي القواعد.
    pub total_rules: usize,
    /// القواعد المفعّلة.
    pub enabled_rules: usize,
    /// القواعد المعطّلة.
    pub disabled_rules: usize,
    /// إجمالي عمليات التنفيذ.
    pub total_executions: u64,
    /// إجمالي النجاحات.
    pub total_successes: u64,
    /// إجمالي الفشل.
    pub total_failures: u64,
}

/// محرك القواعد.
pub struct RuleEngine {
    /// القواعد مُفهرسة بالمعرّف.
    rules: RwLock<HashMap<RuleId, Rule>>,
    /// ناقل الأحداث (للاشتراك + نشر الأحداث المشتقة).
    event_bus: Arc<EventBus>,
    /// خدمة الإشعارات (للإجراءات).
    notifications: Option<Arc<nexora_notifications::NotificationService>>,
}

impl std::fmt::Debug for RuleEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuleEngine")
            .field("rules", &self.rules.read().len())
            .finish_non_exhaustive()
    }
}

impl RuleEngine {
    /// إنشاء محرك قواعد جديد.
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            rules: RwLock::new(HashMap::new()),
            event_bus,
            notifications: None,
        }
    }

    /// ربط خدمة الإشعارات (لإجراءات SendInAppNotification).
    pub fn with_notifications(
        mut self,
        svc: Arc<nexora_notifications::NotificationService>,
    ) -> Self {
        self.notifications = Some(svc);
        self
    }

    /// تسجيل قاعدة جديدة.
    pub fn register(&self, rule: Rule) -> RuleResult<RuleId> {
        let id = rule.id.clone();
        let mut rules = self.rules.write();
        if rules.contains_key(&id) {
            return Err(crate::error::RuleError::AlreadyExists(id));
        }
        rules.insert(id.clone(), rule);
        Ok(id)
    }

    /// تحديث قاعدة موجودة.
    pub fn update(&self, rule: Rule) -> RuleResult<()> {
        let mut rules = self.rules.write();
        if !rules.contains_key(&rule.id) {
            return Err(crate::error::RuleError::NotFound(rule.id));
        }
        rules.insert(rule.id.clone(), rule);
        Ok(())
    }

    /// حذف قاعدة (حذف ناعم).
    pub fn delete(&self, id: &RuleId) -> RuleResult<()> {
        let mut rules = self.rules.write();
        if let Some(rule) = rules.get_mut(id) {
            rule.status = RuleStatus::Deleted;
            Ok(())
        } else {
            Err(crate::error::RuleError::NotFound(id.clone()))
        }
    }

    /// الحصول على قاعدة بالمعرّف.
    pub fn get(&self, id: &RuleId) -> Option<Rule> {
        self.rules.read().get(id).cloned()
    }

    /// قائمة كل القواعد (غير المحذوفة).
    pub fn list(&self) -> Vec<Rule> {
        self.rules
            .read()
            .values()
            .filter(|r| r.status != RuleStatus::Deleted)
            .cloned()
            .collect()
    }

    /// قائمة القواعد المفعّلة فقط.
    pub fn list_enabled(&self) -> Vec<Rule> {
        self.rules
            .read()
            .values()
            .filter(|r| r.is_enabled())
            .cloned()
            .collect()
    }

    /// تفعيل قاعدة.
    pub fn enable(&self, id: &RuleId) -> RuleResult<()> {
        let mut rules = self.rules.write();
        match rules.get_mut(id) {
            Some(r) => {
                r.enable();
                Ok(())
            }
            None => Err(crate::error::RuleError::NotFound(id.clone())),
        }
    }

    /// تعطيل قاعدة.
    pub fn disable(&self, id: &RuleId) -> RuleResult<()> {
        let mut rules = self.rules.write();
        match rules.get_mut(id) {
            Some(r) => {
                r.disable();
                Ok(())
            }
            None => Err(crate::error::RuleError::NotFound(id.clone())),
        }
    }

    /// معالجة حدث — تقيّم كل القواعد المفعّلة وتنفّذ المتطابقة.
    ///
    /// هذه الدالة هي قلب المحرك. تُستدعى عند نشر كل حدث.
    pub async fn process_event(&self, event: &Event) -> Vec<(RuleId, RuleResult<()>)> {
        let matching_rules: Vec<Rule> = {
            let rules = self.rules.read();
            rules
                .values()
                .filter(|r| r.is_enabled())
                .filter(|r| r.condition.evaluate(event).map(|c| c == crate::condition::ConditionResult::Matched).unwrap_or(false))
                .cloned()
                .collect()
        };

        let mut results = Vec::new();
        for mut rule in matching_rules {
            let mut all_success = true;
            for action in &rule.actions {
                let result = action
                    .execute(event, Some(&self.event_bus), self.notifications.as_ref())
                    .await;
                if result.is_err() {
                    all_success = false;
                    break;
                }
            }

            // تحديث الإحصائيات.
            {
                let mut rules = self.rules.write();
                if let Some(r) = rules.get_mut(&rule.id) {
                    r.record_execution(all_success);
                }
            }
            rule.record_execution(all_success);

            if all_success {
                results.push((rule.id, Ok(())));
            } else {
                results.push((
                    rule.id,
                    Err(crate::error::RuleError::ActionFailed(
                        "فشل تنفيذ أحد الإجراءات".into(),
                    )),
                ));
            }
        }
        results
    }

    /// الاشتراك في ناقل الأحداث ومعالجتها تلقائياً.
    ///
    /// يُرجع `EventSubscriber` — احتفظ به لإبقاء الاشتراك حياً.
    pub fn start_auto_processing(&self) -> EventSubscriber {
        let subscriber = self.event_bus.subscribe("");
        // في الإنتاج، سننشر مهمة tokio لقراءة من subscriber ومعالجة الأحداث.
        // للتنفيذ المرجعي، نُرجع الـ subscriber فقط.
        subscriber
    }

    /// إحصائيات المحرك.
    pub fn stats(&self) -> RuleEngineStats {
        let rules = self.rules.read();
        let total = rules
            .values()
            .filter(|r| r.status != RuleStatus::Deleted)
            .count();
        let enabled = rules.values().filter(|r| r.is_enabled()).count();
        let disabled = rules
            .values()
            .filter(|r| r.status == RuleStatus::Disabled)
            .count();
        let total_executions: u64 = rules.values().map(|r| r.execution_count).sum();
        let total_successes: u64 = rules.values().map(|r| r.success_count).sum();
        let total_failures: u64 = rules.values().map(|r| r.failure_count).sum();
        RuleEngineStats {
            total_rules: total,
            enabled_rules: enabled,
            disabled_rules: disabled,
            total_executions,
            total_successes,
            total_failures,
        }
    }

    /// عدد القواعد.
    pub fn rule_count(&self) -> usize {
        self.rules
            .read()
            .values()
            .filter(|r| r.status != RuleStatus::Deleted)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::condition::Condition;
    use crate::rule::Rule;
    use nexora_core::events::{Event, EventPayload};

    fn make_event(name: &str, payload: &str) -> Event {
        Event {
            id: 1,
            name: name.to_string(),
            payload: EventPayload::Text(payload.to_string()),
            timestamp: 0,
        }
    }

    fn make_engine() -> RuleEngine {
        let bus = Arc::new(EventBus::new());
        RuleEngine::new(bus)
    }

    #[tokio::test]
    async fn register_and_list() {
        let engine = make_engine();
        let rule = Rule::new(
            "test",
            Condition::always(),
            vec![Action::log("info", "test")],
        );
        let id = engine.register(rule).unwrap();
        assert_eq!(engine.rule_count(), 1);
        assert!(engine.get(&id).is_some());
    }

    #[tokio::test]
    async fn delete_rule() {
        let engine = make_engine();
        let rule = Rule::new("test", Condition::always(), vec![]);
        let id = engine.register(rule).unwrap();
        assert_eq!(engine.rule_count(), 1);
        engine.delete(&id).unwrap();
        assert_eq!(engine.rule_count(), 0);
    }

    #[tokio::test]
    async fn enable_disable() {
        let engine = make_engine();
        let rule = Rule::new("test", Condition::always(), vec![]);
        let id = engine.register(rule).unwrap();
        engine.disable(&id).unwrap();
        assert_eq!(engine.list_enabled().len(), 0);
        engine.enable(&id).unwrap();
        assert_eq!(engine.list_enabled().len(), 1);
    }

    #[tokio::test]
    async fn process_event_matching_rule() {
        let engine = make_engine();
        let rule = Rule::new(
            "billing-alert",
            Condition::event_name_matches("billing.*"),
            vec![Action::log("info", "billing event: {{trigger.name}}")],
        );
        engine.register(rule).unwrap();

        let event = make_event("billing.payment.failed", "amount: 100");
        let results = engine.process_event(&event).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_ok());
    }

    #[tokio::test]
    async fn process_event_non_matching_rule() {
        let engine = make_engine();
        let rule = Rule::new(
            "auth-alert",
            Condition::event_name_matches("auth.*"),
            vec![Action::log("info", "auth event")],
        );
        engine.register(rule).unwrap();

        let event = make_event("billing.payment", "");
        let results = engine.process_event(&event).await;
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn process_event_publishes_derived_event() {
        let bus = Arc::new(EventBus::new());
        let engine = RuleEngine::new(bus.clone());
        let rule = Rule::new(
            "derive",
            Condition::always(),
            vec![Action::publish_event("derived.event", "from: {{trigger.name}}")],
        );
        engine.register(rule).unwrap();

        let event = make_event("source.event", "");
        engine.process_event(&event).await;
        // الحدث المشتق فقط يُنشر (الحدث المُشغِّل لا يُنشر بواسطة process_event).
        assert_eq!(bus.published_count(), 1);
    }

    #[tokio::test]
    async fn process_event_sends_notification() {
        let bus = Arc::new(EventBus::new());
        let svc = Arc::new(nexora_notifications::NotificationService::new());
        let engine = RuleEngine::new(bus).with_notifications(svc.clone());
        let rule = Rule::new(
            "notify",
            Condition::payload_contains("failed"),
            vec![Action::send_in_app_notification(
                "user-1",
                "فشل: {{trigger.name}}",
                "التفاصيل: {{trigger.payload}}",
            )],
        );
        engine.register(rule).unwrap();

        let event = make_event("billing.payment", "status: failed");
        engine.process_event(&event).await;
        assert_eq!(svc.in_app_store().count("user-1"), 1);
    }

    #[tokio::test]
    async fn disabled_rule_not_processed() {
        let engine = make_engine();
        let rule = Rule::new("test", Condition::always(), vec![Action::log("info", "x")]);
        let id = engine.register(rule).unwrap();
        engine.disable(&id).unwrap();

        let event = make_event("any.event", "");
        let results = engine.process_event(&event).await;
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn multiple_rules_one_event() {
        let engine = make_engine();
        engine
            .register(Rule::new(
                "r1",
                Condition::always(),
                vec![Action::log("info", "r1")],
            ))
            .unwrap();
        engine
            .register(Rule::new(
                "r2",
                Condition::always(),
                vec![Action::log("info", "r2")],
            ))
            .unwrap();

        let event = make_event("test", "");
        let results = engine.process_event(&event).await;
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn stats_track_executions() {
        let engine = make_engine();
        engine
            .register(Rule::new(
                "test",
                Condition::always(),
                vec![Action::log("info", "x")],
            ))
            .unwrap();

        engine.process_event(&make_event("e1", "")).await;
        engine.process_event(&make_event("e2", "")).await;

        let stats = engine.stats();
        assert_eq!(stats.total_rules, 1);
        assert_eq!(stats.enabled_rules, 1);
        assert_eq!(stats.total_executions, 2);
        assert_eq!(stats.total_successes, 2);
    }

    #[tokio::test]
    async fn action_failure_records_failure() {
        let engine = make_engine();
        // إجراء PublishEvent بدون EventBus متوفر سيفشل.
        // لكننا نمرر EventBus، فلنستخدم إجراء يتطلب خدمة غير متوفرة.
        let rule = Rule::new(
            "test",
            Condition::always(),
            vec![Action::send_in_app_notification("u", "t", "b")],
        );
        engine.register(rule).unwrap();

        let event = make_event("e", "");
        let results = engine.process_event(&event).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_err());

        let stats = engine.stats();
        assert_eq!(stats.total_failures, 1);
    }

    #[tokio::test]
    async fn update_existing_rule() {
        let engine = make_engine();
        let mut rule = Rule::new("test", Condition::always(), vec![]);
        let id = engine.register(rule.clone()).unwrap();
        rule.name = "updated".into();
        engine.update(rule).unwrap();
        assert_eq!(engine.get(&id).unwrap().name, "updated");
    }

    #[tokio::test]
    async fn update_nonexistent_fails() {
        let engine = make_engine();
        let rule = Rule::new("test", Condition::always(), vec![]);
        assert!(engine.update(rule).is_err());
    }

    #[tokio::test]
    async fn register_duplicate_fails() {
        let engine = make_engine();
        let rule = Rule::new("test", Condition::always(), vec![]);
        let id = engine.register(rule.clone()).unwrap();
        // محاولة تسجيل نفس القاعدة بنفس المعرّف.
        let mut dup = rule;
        dup.id = id;
        assert!(engine.register(dup).is_err());
    }
}
