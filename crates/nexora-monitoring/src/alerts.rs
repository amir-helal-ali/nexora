//! تنبيهات الأداء — قواعد عتبة تُطلق تنبيهات عند تجاوزها.
//!
//! مثلاً: "إذا تجاوز معدل الخطأ 5%، أطلق تنبيه حرج".

use crate::metrics::MetricsCollector;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use time::OffsetDateTime;

/// نوع عتبة الأداء.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdType {
    /// معدل الخطأ.
    ErrorRate,
    /// متوسط زمن الاستجابة (ميكروثانية).
    AvgLatency,
    /// إجمالي الطلبات (للكشف عن ارتفاع مفاجئ).
    TotalRequests,
    /// عدد المسارات المتتبّعة.
    TrackedPaths,
}

impl ThresholdType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ErrorRate => "error_rate",
            Self::AvgLatency => "avg_latency",
            Self::TotalRequests => "total_requests",
            Self::TrackedPaths => "tracked_paths",
        }
    }
}

/// مستوى التنبيه.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

impl AlertLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

/// قاعدة تنبيه أداء.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceRule {
    /// معرّف فريد.
    pub id: String,
    /// اسم العرض.
    pub name: String,
    /// نوع العتبة.
    pub threshold_type: ThresholdType,
    /// قيمة العتبة (معدل الخطأ: 0.0-1.0، الزمن: ميكروثانية).
    pub threshold: f64,
    /// مستوى التنبيه.
    pub level: AlertLevel,
    /// هل القاعدة مفعّلة؟
    pub enabled: bool,
    /// فترة التهدئة (ثوانٍ) — لا تكرر التنبيه خلالها.
    pub cooldown_seconds: u64,
    /// آخر إطلاق (unix nanos).
    #[serde(skip)]
    pub last_fired: Option<i64>,
}

impl PerformanceRule {
    pub fn new(
        name: impl Into<String>,
        threshold_type: ThresholdType,
        threshold: f64,
        level: AlertLevel,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            threshold_type,
            threshold,
            level,
            enabled: true,
            cooldown_seconds: 60,
            last_fired: None,
        }
    }

    pub fn with_cooldown(mut self, seconds: u64) -> Self {
        self.cooldown_seconds = seconds;
        self
    }

    /// هل القاعدة في فترة التهدئة؟
    pub fn in_cooldown(&self) -> bool {
        if let Some(last) = self.last_fired {
            let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
            let elapsed_secs = (now - last) / 1_000_000_000;
            elapsed_secs < self.cooldown_seconds as i64
        } else {
            false
        }
    }

    /// تقييم القاعدة مقابل مقاييس.
    pub fn evaluate(&self, collector: &MetricsCollector) -> Option<PerformanceAlert> {
        if !self.enabled || self.in_cooldown() {
            return None;
        }

        let metrics = collector.global_metrics();
        let value = match self.threshold_type {
            ThresholdType::ErrorRate => metrics.error_rate(),
            ThresholdType::AvgLatency => metrics.avg_latency_us() as f64,
            ThresholdType::TotalRequests => metrics.total_requests as f64,
            ThresholdType::TrackedPaths => collector.path_count() as f64,
        };

        if value > self.threshold {
            return Some(PerformanceAlert {
                rule_id: self.id.clone(),
                rule_name: self.name.clone(),
                threshold_type: self.threshold_type,
                threshold: self.threshold,
                current_value: value,
                level: self.level,
                message: format!(
                    "{}: {} = {:.4} (العتبة: {:.4})",
                    self.name,
                    self.threshold_type.as_str(),
                    value,
                    self.threshold
                ),
                fired_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            });
        }

        None
    }
}

/// تنبيه أداء.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAlert {
    pub rule_id: String,
    pub rule_name: String,
    pub threshold_type: ThresholdType,
    pub threshold: f64,
    pub current_value: f64,
    pub level: AlertLevel,
    pub message: String,
    pub fired_at: i64,
}

/// مدير تنبيهات الأداء.
pub struct PerformanceAlerter {
    rules: RwLock<Vec<PerformanceRule>>,
    /// آخر التنبيهات (للاستعراض).
    recent_alerts: RwLock<Vec<PerformanceAlert>>,
    /// حد أقصى للتنبيهات المخزّنة.
    max_recent: usize,
}

impl Default for PerformanceAlerter {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceAlerter {
    pub fn new() -> Self {
        Self {
            rules: RwLock::new(Vec::new()),
            recent_alerts: RwLock::new(Vec::new()),
            max_recent: 100,
        }
    }

    /// تسجيل قاعدة.
    pub fn register(&self, rule: PerformanceRule) -> String {
        let id = rule.id.clone();
        self.rules.write().push(rule);
        id
    }

    /// إنشاء القواعد الافتراضية.
    pub fn with_defaults(self) -> Self {
        self.register(PerformanceRule::new(
            "معدل خطأ عالي",
            ThresholdType::ErrorRate,
            0.05, // 5%
            AlertLevel::Warning,
        ).with_cooldown(30));
        self.register(PerformanceRule::new(
            "معدل خطأ حرج",
            ThresholdType::ErrorRate,
            0.15, // 15%
            AlertLevel::Critical,
        ).with_cooldown(30));
        self.register(PerformanceRule::new(
            "زمن استجابة بطيء",
            ThresholdType::AvgLatency,
            500_000.0, // 500ms
            AlertLevel::Warning,
        ).with_cooldown(60));
        self.register(PerformanceRule::new(
            "زمن استجابة حرج",
            ThresholdType::AvgLatency,
            2_000_000.0, // 2s
            AlertLevel::Critical,
        ).with_cooldown(60));
        self
    }

    /// تقييم كل القواعد.
    pub fn check(&self, collector: &MetricsCollector) -> Vec<PerformanceAlert> {
        let mut alerts = Vec::new();
        let mut rules = self.rules.write();
        for rule in rules.iter_mut() {
            if let Some(alert) = rule.evaluate(collector) {
                rule.last_fired = Some(alert.fired_at);
                alerts.push(alert.clone());
                let mut recent = self.recent_alerts.write();
                recent.push(alert);
                if recent.len() > self.max_recent {
                    recent.remove(0);
                }
            }
        }
        alerts
    }

    /// قائمة القواعد.
    pub fn list_rules(&self) -> Vec<PerformanceRule> {
        self.rules.read().clone()
    }

    /// آخر التنبيهات.
    pub fn recent_alerts(&self) -> Vec<PerformanceAlert> {
        self.recent_alerts.read().clone()
    }

    /// عدد القواعد.
    pub fn rule_count(&self) -> usize {
        self.rules.read().len()
    }

    /// تفعيل/تعطيل قاعدة.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> bool {
        let mut rules = self.rules.write();
        for r in rules.iter_mut() {
            if r.id == id {
                r.enabled = enabled;
                return true;
            }
        }
        false
    }

    /// حذف قاعدة.
    pub fn remove(&self, id: &str) -> bool {
        let mut rules = self.rules.write();
        let before = rules.len();
        rules.retain(|r| r.id != id);
        rules.len() != before
    }

    /// مسح التنبيهات الأخيرة.
    pub fn clear_alerts(&self) {
        self.recent_alerts.write().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn rule_error_rate_triggers() {
        let collector = MetricsCollector::new();
        collector.record("/api/test", Duration::from_micros(1), 200);
        collector.record("/api/test", Duration::from_micros(1), 500);
        collector.record("/api/test", Duration::from_micros(1), 500);
        // error_rate = 2/3 = 0.667

        let rule = PerformanceRule::new("test", ThresholdType::ErrorRate, 0.5, AlertLevel::Warning);
        let alert = rule.evaluate(&collector);
        assert!(alert.is_some());
        assert!(alert.unwrap().current_value > 0.5);
    }

    #[test]
    fn rule_error_rate_below_threshold() {
        let collector = MetricsCollector::new();
        collector.record("/api/test", Duration::from_micros(1), 200);
        collector.record("/api/test", Duration::from_micros(1), 200);
        collector.record("/api/test", Duration::from_micros(1), 500);
        // error_rate = 1/3 = 0.333

        let rule = PerformanceRule::new("test", ThresholdType::ErrorRate, 0.5, AlertLevel::Warning);
        assert!(rule.evaluate(&collector).is_none());
    }

    #[test]
    fn rule_latency_triggers() {
        let collector = MetricsCollector::new();
        collector.record("/api/test", Duration::from_millis(600), 200);
        // avg = 600_000μs

        let rule = PerformanceRule::new("slow", ThresholdType::AvgLatency, 500_000.0, AlertLevel::Warning);
        let alert = rule.evaluate(&collector);
        assert!(alert.is_some());
    }

    #[test]
    fn rule_disabled_does_not_trigger() {
        let collector = MetricsCollector::new();
        collector.record("/api/test", Duration::from_micros(1), 500);

        let mut rule = PerformanceRule::new("test", ThresholdType::ErrorRate, 0.01, AlertLevel::Warning);
        rule.enabled = false;
        assert!(rule.evaluate(&collector).is_none());
    }

    #[test]
    fn rule_cooldown_prevents_repeat() {
        let collector = MetricsCollector::new();
        collector.record("/api/test", Duration::from_micros(1), 500);

        let mut rule = PerformanceRule::new("test", ThresholdType::ErrorRate, 0.01, AlertLevel::Warning);
        rule.cooldown_seconds = 3600; // ساعة

        let alert1 = rule.evaluate(&collector);
        assert!(alert1.is_some());
        rule.last_fired = Some(alert1.unwrap().fired_at);

        let alert2 = rule.evaluate(&collector);
        assert!(alert2.is_none()); // في التهدئة.
    }

    #[test]
    fn alerter_register_and_list() {
        let alerter = PerformanceAlerter::new();
        alerter.register(PerformanceRule::new(
            "test",
            ThresholdType::ErrorRate,
            0.1,
            AlertLevel::Warning,
        ));
        assert_eq!(alerter.rule_count(), 1);
    }

    #[test]
    fn alerter_with_defaults_has_4_rules() {
        let alerter = PerformanceAlerter::new().with_defaults();
        assert_eq!(alerter.rule_count(), 4);
    }

    #[test]
    fn alerter_check_fires_alerts() {
        let alerter = PerformanceAlerter::new().with_defaults();
        let collector = MetricsCollector::new();
        // كل الطلبات فاشلة.
        for _ in 0..10 {
            collector.record("/api/test", Duration::from_micros(1), 500);
        }
        let alerts = alerter.check(&collector);
        // على الأقل تنبيه واحد (معدل خطأ > 5% و > 15%).
        assert!(!alerts.is_empty());
    }

    #[test]
    fn alerter_check_no_alerts_when_healthy() {
        let alerter = PerformanceAlerter::new().with_defaults();
        let collector = MetricsCollector::new();
        collector.record("/api/test", Duration::from_micros(10), 200);
        let alerts = alerter.check(&collector);
        assert!(alerts.is_empty());
    }

    #[test]
    fn alerter_recent_alerts() {
        let alerter = PerformanceAlerter::new().with_defaults();
        let collector = MetricsCollector::new();
        for _ in 0..20 {
            collector.record("/api/test", Duration::from_millis(600), 500);
        }
        alerter.check(&collector);
        assert!(!alerter.recent_alerts().is_empty());
    }

    #[test]
    fn alerter_set_enabled() {
        let alerter = PerformanceAlerter::new();
        let id = alerter.register(PerformanceRule::new(
            "test",
            ThresholdType::ErrorRate,
            0.1,
            AlertLevel::Warning,
        ));
        assert!(alerter.set_enabled(&id, false));
        let rules = alerter.list_rules();
        assert!(!rules[0].enabled);
    }

    #[test]
    fn alerter_remove() {
        let alerter = PerformanceAlerter::new();
        let id = alerter.register(PerformanceRule::new(
            "test",
            ThresholdType::ErrorRate,
            0.1,
            AlertLevel::Warning,
        ));
        assert!(alerter.remove(&id));
        assert_eq!(alerter.rule_count(), 0);
    }

    #[test]
    fn alerter_clear_alerts() {
        let alerter = PerformanceAlerter::new().with_defaults();
        let collector = MetricsCollector::new();
        for _ in 0..20 {
            collector.record("/api/test", Duration::from_micros(1), 500);
        }
        alerter.check(&collector);
        assert!(!alerter.recent_alerts().is_empty());
        alerter.clear_alerts();
        assert!(alerter.recent_alerts().is_empty());
    }

    #[test]
    fn threshold_type_as_str() {
        assert_eq!(ThresholdType::ErrorRate.as_str(), "error_rate");
        assert_eq!(ThresholdType::AvgLatency.as_str(), "avg_latency");
    }

    #[test]
    fn alert_level_as_str() {
        assert_eq!(AlertLevel::Info.as_str(), "info");
        assert_eq!(AlertLevel::Warning.as_str(), "warning");
        assert_eq!(AlertLevel::Critical.as_str(), "critical");
    }

    #[test]
    fn rule_with_cooldown() {
        let rule = PerformanceRule::new("test", ThresholdType::ErrorRate, 0.1, AlertLevel::Warning)
            .with_cooldown(120);
        assert_eq!(rule.cooldown_seconds, 120);
    }

    #[test]
    fn serde_roundtrip() {
        let rule = PerformanceRule::new("test", ThresholdType::ErrorRate, 0.1, AlertLevel::Critical)
            .with_cooldown(30);
        let json = serde_json::to_string(&rule).unwrap();
        let back: PerformanceRule = serde_json::from_str(&json).unwrap();
        assert_eq!(rule.id, back.id);
        assert_eq!(rule.threshold, back.threshold);
        assert_eq!(rule.level, back.level);
    }

    #[test]
    fn defaults_include_critical_rules() {
        let alerter = PerformanceAlerter::new().with_defaults();
        let rules = alerter.list_rules();
        assert!(rules.iter().any(|r| r.level == AlertLevel::Critical));
    }
}
