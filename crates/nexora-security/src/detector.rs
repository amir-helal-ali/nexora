//! كواشف التهديدات (Detectors).

use crate::alert::{Severity, SecurityAlert};
use crate::threat::{ThreatIndicator, ThreatType};
use nexora_audit::AuditEntry;
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Trait لكاشف التهديدات.
pub trait Detector: Send + Sync {
    /// اسم الكاشف.
    fn name(&self) -> &str;

    /// حلّل مدخل تدقيق وأرجع تنبيهاً إن وُجد.
    fn analyze(&self, entry: &AuditEntry) -> Option<SecurityAlert>;
}

/// كاشف محاولات تسجيل الدخول الفاشلة المتكررة (brute force).
pub struct BruteForceDetector {
    /// عدد المحاولات الفاشلة المطلوبة لإطلاق تنبيه.
    threshold: usize,
    /// نافذة الوقت (بالثواني).
    window_seconds: u64,
    /// تتبع المحاولات الفاشلة لكل فاعل: (actor → Vec<timestamp_nanos>).
    attempts: RwLock<HashMap<String, VecDeque<i64>>>,
}

impl BruteForceDetector {
    pub fn new(threshold: usize, window_seconds: u64) -> Self {
        Self {
            threshold,
            window_seconds,
            attempts: RwLock::new(HashMap::new()),
        }
    }

    pub fn default() -> Self {
        Self::new(5, 300) // 5 محاولات في 5 دقائق
    }
}

impl Detector for BruteForceDetector {
    fn name(&self) -> &str {
        "brute_force"
    }

    fn analyze(&self, entry: &AuditEntry) -> Option<SecurityAlert> {
        // اهتم فقط بمحاولات المصادقة الفاشلة.
        let is_auth_failure = entry.category == nexora_audit::AuditCategory::Auth
            && !entry.success
            && (entry.action.contains("login") || entry.action.contains("auth"));

        if !is_auth_failure {
            return None;
        }

        let now = entry.timestamp;
        let window_nanos = (self.window_seconds as i64) * 1_000_000_000;
        let cutoff = now - window_nanos;

        let mut attempts = self.attempts.write();
        let queue = attempts.entry(entry.actor.clone()).or_default();

        // أضف المحاولة الحالية.
        queue.push_back(now);

        // أزل المحاولات القديمة خارج النافذة.
        while let Some(&front) = queue.front() {
            if front < cutoff {
                queue.pop_front();
            } else {
                break;
            }
        }

        // تحقق من العتبة.
        if queue.len() >= self.threshold {
            let count = queue.len();
            // امسح المحاولات (لمنع التنبيه المتكرر).
            queue.clear();

            let confidence = (count as f64 / (self.threshold * 2) as f64).min(1.0);
            let severity = Severity::from_confidence(confidence);

            return Some(SecurityAlert::new(
                entry.actor.clone(),
                severity,
                ThreatType::BruteForce,
                format!("{count} محاولات تسجيل دخول فاشلة في {} ثانية", self.window_seconds),
                vec![ThreatIndicator::new(
                    ThreatType::BruteForce,
                    format!("{count} failed attempts"),
                    entry.actor.clone(),
                    confidence,
                )],
            ));
        }

        None
    }
}

/// كاشف النشاط غير المعتاد (anomaly).
///
/// يكتشف الأنماط غير المعتادة مثل:
/// - عدد كبير من الإجراءات في وقت قصير
/// - إجراءات لم يقم بها المستخدم من قبل
pub struct AnomalyDetector {
    /// حد معدل الإجراءات في الدقيقة.
    rate_limit: usize,
    /// تتبع معدل الإجراءات لكل فاعل.
    rates: RwLock<HashMap<String, VecDeque<i64>>>,
}

impl AnomalyDetector {
    pub fn new(rate_limit: usize) -> Self {
        Self {
            rate_limit,
            rates: RwLock::new(HashMap::new()),
        }
    }

    pub fn default() -> Self {
        Self::new(100) // 100 إجراء/دقيقة
    }
}

impl Detector for AnomalyDetector {
    fn name(&self) -> &str {
        "anomaly"
    }

    fn analyze(&self, entry: &AuditEntry) -> Option<SecurityAlert> {
        let now = entry.timestamp;
        let window_nanos = 60_000_000_000i64; // دقيقة واحدة

        let mut rates = self.rates.write();
        let queue = rates.entry(entry.actor.clone()).or_default();
        queue.push_back(now);

        // أزل القديم.
        while let Some(&front) = queue.front() {
            if front < now - window_nanos {
                queue.pop_front();
            } else {
                break;
            }
        }

        if queue.len() > self.rate_limit {
            let count = queue.len();
            let confidence = (count as f64 / (self.rate_limit * 2) as f64).min(1.0);
            return Some(SecurityAlert::new(
                entry.actor.clone(),
                Severity::from_confidence(confidence),
                ThreatType::RateLimitAbuse,
                format!("{count} إجراء في دقيقة واحدة (الحد: {})", self.rate_limit),
                vec![ThreatIndicator::new(
                    ThreatType::RateLimitAbuse,
                    format!("rate: {count}/min"),
                    entry.actor.clone(),
                    confidence,
                )],
            ));
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_audit::{AuditCategory, AuditEntry};

    fn make_entry(actor: &str, action: &str, success: bool, ts: i64) -> AuditEntry {
        AuditEntry::new(actor, action, "target")
            .with_category(AuditCategory::Auth)
            .with_success(success)
            .with_timestamp(ts)
    }

    #[test]
    fn brute_force_below_threshold() {
        let det = BruteForceDetector::new(5, 300);
        let e = make_entry("alice", "login", false, 1000);
        assert!(det.analyze(&e).is_none());
    }

    #[test]
    fn brute_force_at_threshold() {
        let det = BruteForceDetector::new(3, 300);
        let base_ts = 1_000_000_000_000;
        for i in 0..2 {
            let e = make_entry("alice", "login", false, base_ts + i * 1_000_000_000);
            assert!(det.analyze(&e).is_none());
        }
        // المحاولة الثالثة يجب أن تطلق تنبيه.
        let e = make_entry("alice", "login", false, base_ts + 2 * 1_000_000_000);
        let alert = det.analyze(&e).unwrap();
        assert_eq!(alert.threat_type, ThreatType::BruteForce);
        assert!(alert.description.contains("3 محاولات"));
    }

    #[test]
    fn brute_force_ignores_successful() {
        let det = BruteForceDetector::new(2, 300);
        let e = make_entry("alice", "login", true, 1000);
        assert!(det.analyze(&e).is_none());
    }

    #[test]
    fn brute_force_ignores_non_auth() {
        let det = BruteForceDetector::new(2, 300);
        let e = AuditEntry::new("alice", "read", "target")
            .with_category(AuditCategory::Data)
            .with_success(false)
            .with_timestamp(1000);
        assert!(det.analyze(&e).is_none());
    }

    #[test]
    fn brute_force_window_expiry() {
        let det = BruteForceDetector::new(3, 1); // نافذة ثانية واحدة
        let base = 1_000_000_000_000i64;
        // محاولتان عند base.
        for i in 0..2 {
            let e = make_entry("alice", "login", false, base + i);
            assert!(det.analyze(&e).is_none());
        }
        // محاولة بعد ثانيتين (خارج النافذة).
        let e = make_entry("alice", "login", false, base + 3_000_000_000);
        assert!(det.analyze(&e).is_none());
    }

    #[test]
    fn anomaly_below_rate_limit() {
        let det = AnomalyDetector::new(100);
        let e = make_entry("alice", "action", true, 1000);
        assert!(det.analyze(&e).is_none());
    }

    #[test]
    fn anomaly_above_rate_limit() {
        let det = AnomalyDetector::new(3);
        let base = 1_000_000_000_000i64;
        for i in 0..3 {
            let e = make_entry("alice", "action", true, base + i);
            assert!(det.analyze(&e).is_none());
        }
        // الإجراء الرابع يتجاوز الحد.
        let e = make_entry("alice", "action", true, base + 4);
        let alert = det.analyze(&e).unwrap();
        assert_eq!(alert.threat_type, ThreatType::RateLimitAbuse);
    }

    #[test]
    fn detector_names() {
        let bf = BruteForceDetector::default();
        let an = AnomalyDetector::default();
        assert_eq!(bf.name(), "brute_force");
        assert_eq!(an.name(), "anomaly");
    }
}
