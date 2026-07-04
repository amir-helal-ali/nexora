//! محرك الأمان — ينسق الكواشف ويدير التنبيهات.

use crate::alert::{AlertStatus, SecurityAlert, Severity};
use crate::detector::{AnomalyDetector, BruteForceDetector, Detector};
use crate::threat::ThreatType;
use nexora_audit::AuditEntry;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// إحصائيات الأمان.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SecurityStats {
    /// إجمالي التنبيهات.
    pub total_alerts: usize,
    /// تنبيهات نشطة.
    pub active_alerts: usize,
    /// تنبيهات محلة.
    pub resolved_alerts: usize,
    /// تنبيهات حرجة.
    pub critical_alerts: usize,
    /// تنبيهات عالية.
    pub high_alerts: usize,
    /// عدد التحليلات.
    pub total_analyses: u64,
    /// آخر تنبيه (unix nanos).
    pub last_alert_at: Option<i64>,
}

/// محرك الأمان.
pub struct SecurityEngine {
    /// الكواشف المسجّلة.
    detectors: Vec<Box<dyn Detector>>,
    /// التنبيهات النشطة والمحلة.
    alerts: RwLock<HashMap<String, SecurityAlert>>,
}

impl Default for SecurityEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityEngine {
    /// إنشاء محرك أمان بالكواشف الافتراضية.
    pub fn new() -> Self {
        Self {
            detectors: vec![
                Box::new(BruteForceDetector::default()),
                Box::new(AnomalyDetector::default()),
            ],
            alerts: RwLock::new(HashMap::new()),
        }
    }

    /// إنشاء محرك بكواشف مخصصة.
    pub fn with_detectors(detectors: Vec<Box<dyn Detector>>) -> Self {
        Self {
            detectors,
            alerts: RwLock::new(HashMap::new()),
        }
    }

    /// تحليل مدخل تدقيق — يُرجع التنبيه إن أُنشئ.
    pub fn analyze(&self, entry: &AuditEntry) -> Option<SecurityAlert> {
        for detector in &self.detectors {
            if let Some(alert) = detector.analyze(entry) {
                let alert_id = alert.id.clone();
                let alert_clone = alert.clone();
                self.alerts.write().insert(alert_id, alert);
                return Some(alert_clone);
            }
        }
        None
    }

    /// الحصول على تنبيه بالمعرّف.
    pub fn get_alert(&self, id: &str) -> Option<SecurityAlert> {
        self.alerts.read().get(id).cloned()
    }

    /// قائمة كل التنبيهات.
    pub fn list_alerts(&self) -> Vec<SecurityAlert> {
        self.alerts.read().values().cloned().collect()
    }

    /// قائمة التنبيهات النشطة فقط.
    pub fn list_active_alerts(&self) -> Vec<SecurityAlert> {
        self.alerts
            .read()
            .values()
            .filter(|a| a.status == AlertStatus::Active)
            .cloned()
            .collect()
    }

    /// قائمة التنبيهات حسب الخطورة.
    pub fn list_by_severity(&self, severity: Severity) -> Vec<SecurityAlert> {
        self.alerts
            .read()
            .values()
            .filter(|a| a.severity == severity)
            .cloned()
            .collect()
    }

    /// حل تنبيه.
    pub fn resolve_alert(&self, id: &str) -> bool {
        let mut alerts = self.alerts.write();
        if let Some(a) = alerts.get_mut(id) {
            a.resolve();
            return true;
        }
        false
    }

    /// تجاهل تنبيه.
    pub fn dismiss_alert(&self, id: &str) -> bool {
        let mut alerts = self.alerts.write();
        if let Some(a) = alerts.get_mut(id) {
            a.dismiss();
            return true;
        }
        false
    }

    /// إحصائيات الأمان.
    pub fn stats(&self) -> SecurityStats {
        let alerts = self.alerts.read();
        let total = alerts.len();
        let active = alerts.values().filter(|a| a.status == AlertStatus::Active).count();
        let resolved = alerts.values().filter(|a| a.status == AlertStatus::Resolved).count();
        let critical = alerts.values().filter(|a| a.severity == Severity::Critical).count();
        let high = alerts.values().filter(|a| a.severity == Severity::High).count();
        let last = alerts.values().map(|a| a.created_at).max();

        SecurityStats {
            total_alerts: total,
            active_alerts: active,
            resolved_alerts: resolved,
            critical_alerts: critical,
            high_alerts: high,
            total_analyses: 0, // يُحدّث خارجياً
            last_alert_at: last,
        }
    }

    /// عدد الكواشف المسجّلة.
    pub fn detector_count(&self) -> usize {
        self.detectors.len()
    }

    /// مسح التنبيهات المحلة/المتجاهلة.
    pub fn clear_resolved(&self) -> usize {
        let mut alerts = self.alerts.write();
        let before = alerts.len();
        alerts.retain(|_, a| a.status == AlertStatus::Active || a.status == AlertStatus::Investigating);
        before - alerts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_audit::{AuditCategory, AuditEntry};

    fn make_failed_login(actor: &str, ts: i64) -> AuditEntry {
        AuditEntry::new(actor, "login", "session")
            .with_category(AuditCategory::Auth)
            .with_success(false)
            .with_timestamp(ts)
    }

    #[test]
    fn analyze_no_threat() {
        let engine = SecurityEngine::new();
        let entry = AuditEntry::new("alice", "read", "data")
            .with_category(AuditCategory::Data)
            .with_success(true)
            .with_timestamp(1000);
        assert!(engine.analyze(&entry).is_none());
    }

    #[test]
    fn analyze_brute_force() {
        let engine = SecurityEngine::new();
        let base = 1_000_000_000_000i64;
        for i in 0..4 {
            let e = make_failed_login("attacker", base + i * 1_000_000_000);
            engine.analyze(&e);
        }
        // المحاولة الخامسة تطلق تنبيه.
        let e = make_failed_login("attacker", base + 4 * 1_000_000_000);
        let alert = engine.analyze(&e).unwrap();
        assert_eq!(alert.threat_type, ThreatType::BruteForce);
        assert!(engine.list_alerts().len() >= 1);
    }

    #[test]
    fn resolve_alert() {
        let engine = SecurityEngine::new();
        let base = 1_000_000_000_000i64;
        for i in 0..5 {
            let e = make_failed_login("bob", base + i * 1_000_000_000);
            if let Some(alert) = engine.analyze(&e) {
                assert!(engine.resolve_alert(&alert.id));
                let resolved = engine.get_alert(&alert.id).unwrap();
                assert_eq!(resolved.status, AlertStatus::Resolved);
            }
        }
    }

    #[test]
    fn dismiss_alert() {
        let engine = SecurityEngine::new();
        let base = 1_000_000_000_000i64;
        for i in 0..5 {
            let e = make_failed_login("carol", base + i * 1_000_000_000);
            if let Some(alert) = engine.analyze(&e) {
                engine.dismiss_alert(&alert.id);
                let dismissed = engine.get_alert(&alert.id).unwrap();
                assert_eq!(dismissed.status, AlertStatus::Dismissed);
            }
        }
    }

    #[test]
    fn list_active_alerts() {
        let engine = SecurityEngine::new();
        let base = 1_000_000_000_000i64;
        for i in 0..5 {
            let e = make_failed_login("dave", base + i * 1_000_000_000);
            engine.analyze(&e);
        }
        let active = engine.list_active_alerts();
        assert!(active.len() >= 1);
        for a in &active {
            assert_eq!(a.status, AlertStatus::Active);
        }
    }

    #[test]
    fn stats_track_alerts() {
        let engine = SecurityEngine::new();
        let base = 1_000_000_000_000i64;
        for i in 0..5 {
            let e = make_failed_login("eve", base + i * 1_000_000_000);
            engine.analyze(&e);
        }
        let stats = engine.stats();
        assert!(stats.total_alerts >= 1);
        assert!(stats.active_alerts >= 1);
    }

    #[test]
    fn clear_resolved() {
        let engine = SecurityEngine::new();
        let base = 1_000_000_000_000i64;
        let mut alert_id = String::new();
        for i in 0..5 {
            let e = make_failed_login("frank", base + i * 1_000_000_000);
            if let Some(a) = engine.analyze(&e) {
                alert_id = a.id;
            }
        }
        engine.resolve_alert(&alert_id);
        let cleared = engine.clear_resolved();
        assert!(cleared >= 1);
    }

    #[test]
    fn detector_count() {
        let engine = SecurityEngine::new();
        assert_eq!(engine.detector_count(), 2);
    }

    #[test]
    fn get_nonexistent_alert() {
        let engine = SecurityEngine::new();
        assert!(engine.get_alert("nonexistent").is_none());
    }

    #[test]
    fn resolve_nonexistent_returns_false() {
        let engine = SecurityEngine::new();
        assert!(!engine.resolve_alert("nonexistent"));
    }

    #[test]
    fn dismiss_nonexistent_returns_false() {
        let engine = SecurityEngine::new();
        assert!(!engine.dismiss_alert("nonexistent"));
    }
}

impl SecurityEngine {
    /// ربط محرك الأمان بخدمة الإشعارات.
    /// عند إنشاء تنبيه حرج/عالي، يُرسل إشعار تلقائياً.
    pub fn analyze_and_notify(
        &self,
        entry: &nexora_audit::AuditEntry,
        notifications: Option<&std::sync::Arc<nexora_notifications::NotificationService>>,
    ) -> Option<SecurityAlert> {
        let alert = self.analyze(entry)?;

        // أرسل إشعاراً للتنبيهات الحرجة والعالية.
        if matches!(alert.severity, Severity::Critical | Severity::High) {
            if let Some(svc) = notifications {
                let _ = svc.send_in_app(
                    &alert.actor,
                    &format!("⚠️ تنبيه أمني: {}", alert.severity),
                    &format!(
                        "النوع: {}\nالوصف: {}\nالفاعل: {}",
                        alert.threat_type, alert.description, alert.actor
                    ),
                    None,
                );
            }
        }

        Some(alert)
    }
}
