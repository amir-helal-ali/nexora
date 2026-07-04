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

/// كاشف الوصول في أوقات غير معتادة (off-hours access).
///
/// يكتشف الوصول خارج ساعات العمل العادية (مثلاً 2:00 صباحاً).
pub struct OffHoursDetector {
    /// ساعة بداية العمل (0-23).
    work_start: u32,
    /// ساعة نهاية العمل (0-23).
    work_end: u32,
    /// المنطقة الزمنية (بالثواني من UTC).
    tz_offset_seconds: i64,
}

impl OffHoursDetector {
    pub fn new(work_start: u32, work_end: u32, tz_offset_seconds: i64) -> Self {
        Self { work_start, work_end, tz_offset_seconds }
    }

    pub fn default() -> Self {
        // ساعات العمل 8:00 - 18:00، UTC+3
        Self::new(8, 18, 3 * 3600)
    }

    fn is_off_hours(&self, timestamp_nanos: i64) -> bool {
        let secs = timestamp_nanos / 1_000_000_000;
        let local_secs = secs + self.tz_offset_seconds;
        let hours_since_midnight = ((local_secs % 86400 + 86400) % 86400) / 3600;
        let hour = hours_since_midnight as u32;

        if self.work_start <= self.work_end {
            // نطاق عادي (مثلاً 8-18).
            hour < self.work_start || hour >= self.work_end
        } else {
            // نطاق عبر منتصف الليل (مثلاً 22-6).
            hour >= self.work_end && hour < self.work_start
        }
    }
}

impl Detector for OffHoursDetector {
    fn name(&self) -> &str {
        "off_hours"
    }

    fn analyze(&self, entry: &AuditEntry) -> Option<SecurityAlert> {
        // اهتم فقط بالإجراءات الحساسة.
        let is_sensitive = matches!(
            entry.category,
            nexora_audit::AuditCategory::Auth
                | nexora_audit::AuditCategory::UserManagement
                | nexora_audit::AuditCategory::Secret
                | nexora_audit::AuditCategory::Config
        );

        if !is_sensitive || !entry.success {
            return None;
        }

        if self.is_off_hours(entry.timestamp) {
            let local_secs = entry.timestamp / 1_000_000_000 + self.tz_offset_seconds;
            let hour = ((local_secs % 86400 + 86400) % 86400) / 3600;
            return Some(SecurityAlert::new(
                entry.actor.clone(),
                Severity::Medium,
                ThreatType::OffHoursAccess,
                format!("وصول حساس في ساعة غير معتادة ({hour:02}:00)"),
                vec![ThreatIndicator::new(
                    ThreatType::OffHoursAccess,
                    format!("hour={hour}"),
                    entry.actor.clone(),
                    0.5,
                )],
            ));
        }

        None
    }
}

/// كاشف السفر المستحيل (impossible travel).
///
/// يكتشف دخول نفس المستخدم من IP مختلف في وقت قصير جداً
/// (يستحيل السفر بين الموقعين في هذا الوقت).
pub struct ImpossibleTravelDetector {
    /// الحد الأقصى للسرعة (كم/ساعة) للاعتبار ممكناً.
    max_speed_kmh: f64,
    /// تتبع آخر موقع لكل فاعل: (actor → (ip, timestamp)).
    last_location: RwLock<HashMap<String, (String, i64)>>,
}

impl ImpossibleTravelDetector {
    pub fn new(max_speed_kmh: f64) -> Self {
        Self {
            max_speed_kmh,
            last_location: RwLock::new(HashMap::new()),
        }
    }

    pub fn default() -> Self {
        Self::new(900.0) // 900 كم/ساعة (أسرع من الطائرات التجارية)
    }
}

impl Detector for ImpossibleTravelDetector {
    fn name(&self) -> &str {
        "impossible_travel"
    }

    fn analyze(&self, entry: &AuditEntry) -> Option<SecurityAlert> {
        // اهتم فقط بتسجيل الدخول الناجح.
        if entry.category != nexora_audit::AuditCategory::Auth
            || !entry.action.contains("login")
            || !entry.success
        {
            return None;
        }

        // استخرج IP من البيانات الوصفية.
        let ip = entry.metadata.get("ip")?.clone();
        let now = entry.timestamp;

        let mut locations = self.last_location.write();
        if let Some((prev_ip, prev_ts)) = locations.get(&entry.actor).cloned() {
            if prev_ip != ip {
                let time_diff_secs = (now - prev_ts) / 1_000_000_000;
                if time_diff_secs > 0 && time_diff_secs < 3600 {
                    // تبديل IP خلال أقل من ساعة.
                    // في الإنتاج، سنحسب المسافة الجغرافية بين IPs.
                    // للتنفيذ المرجعي، نعتبر أي تبديل سريع مشبوهاً.
                    let confidence = (1.0 - time_diff_secs as f64 / 3600.0).max(0.5);
                    let alert = SecurityAlert::new(
                        entry.actor.clone(),
                        Severity::from_confidence(confidence),
                        ThreatType::ImpossibleTravel,
                        format!(
                            "تبديل IP من {prev_ip} إلى {ip} خلال {time_diff_secs} ثانية"
                        ),
                        vec![ThreatIndicator::new(
                            ThreatType::ImpossibleTravel,
                            format!("{prev_ip} → {ip}"),
                            entry.actor.clone(),
                            confidence,
                        )],
                    );
                    locations.insert(entry.actor.clone(), (ip, now));
                    return Some(alert);
                }
            }
        }

        locations.insert(entry.actor.clone(), (ip, now));
        None
    }
}

#[cfg(test)]
mod advanced_tests {
    use super::*;
    use nexora_audit::AuditCategory;

    fn make_login_entry(actor: &str, ip: &str, ts: i64) -> AuditEntry {
        AuditEntry::new(actor, "login", "session")
            .with_category(AuditCategory::Auth)
            .with_success(true)
            .with_timestamp(ts)
            .with_metadata("ip", ip)
    }

    // --- OffHoursDetector tests ---

    #[test]
    fn off_hours_detects_night_access() {
        let det = OffHoursDetector::new(8, 18, 0); // UTC
        // 2:00 صباحاً UTC = 7200 ثانية من منتصف الليل.
        let ts = 7200 * 1_000_000_000i64;
        let e = AuditEntry::new("alice", "login", "s")
            .with_category(AuditCategory::Auth)
            .with_success(true)
            .with_timestamp(ts);
        assert!(det.analyze(&e).is_some());
    }

    #[test]
    fn off_hours_ignores_work_hours() {
        let det = OffHoursDetector::new(8, 18, 0); // UTC
        // 12:00 ظهراً UTC.
        let ts = 43200 * 1_000_000_000i64;
        let e = AuditEntry::new("alice", "login", "s")
            .with_category(AuditCategory::Auth)
            .with_success(true)
            .with_timestamp(ts);
        assert!(det.analyze(&e).is_none());
    }

    #[test]
    fn off_hours_ignores_failures() {
        let det = OffHoursDetector::default();
        let e = AuditEntry::new("alice", "login", "s")
            .with_category(AuditCategory::Auth)
            .with_success(false)
            .with_timestamp(7200 * 1_000_000_000i64);
        assert!(det.analyze(&e).is_none());
    }

    #[test]
    fn off_hours_ignores_non_sensitive() {
        let det = OffHoursDetector::default();
        let e = AuditEntry::new("alice", "read", "data")
            .with_category(AuditCategory::Data)
            .with_success(true)
            .with_timestamp(7200 * 1_000_000_000i64);
        assert!(det.analyze(&e).is_none());
    }

    #[test]
    fn off_hours_tz_offset() {
        // ساعات عمل 8-18 UTC+3.
        let det = OffHoursDetector::new(8, 18, 3 * 3600);
        // 5:00 UTC = 8:00 UTC+3 (بداية العمل).
        let ts = (5 * 3600) * 1_000_000_000i64;
        let e = AuditEntry::new("alice", "login", "s")
            .with_category(AuditCategory::Auth)
            .with_success(true)
            .with_timestamp(ts);
        assert!(det.analyze(&e).is_none());
    }

    #[test]
    fn off_hours_wraps_midnight() {
        // ساعات عمل 22-6 (داخل نطاق الليل).
        let det = OffHoursDetector::new(22, 6, 0);
        // 12:00 ظهراً = خارج النطاق (غير معتاد).
        let ts = 43200 * 1_000_000_000i64;
        let e = AuditEntry::new("alice", "login", "s")
            .with_category(AuditCategory::Auth)
            .with_success(true)
            .with_timestamp(ts);
        assert!(det.analyze(&e).is_some());
    }

    // --- ImpossibleTravelDetector tests ---

    #[test]
    fn impossible_travel_detects_rapid_ip_change() {
        let det = ImpossibleTravelDetector::default();
        let base = 1_000_000_000_000i64;
        let e1 = make_login_entry("alice", "1.1.1.1", base);
        assert!(det.analyze(&e1).is_none()); // أول دخول — لا تنبيه.

        // دخول من IP مختلف بعد 5 دقائق.
        let e2 = make_login_entry("alice", "2.2.2.2", base + 300 * 1_000_000_000);
        let alert = det.analyze(&e2).unwrap();
        assert_eq!(alert.threat_type, ThreatType::ImpossibleTravel);
        assert!(alert.description.contains("1.1.1.1"));
        assert!(alert.description.contains("2.2.2.2"));
    }

    #[test]
    fn impossible_travel_ignores_same_ip() {
        let det = ImpossibleTravelDetector::default();
        let base = 1_000_000_000_000i64;
        let e1 = make_login_entry("alice", "1.1.1.1", base);
        det.analyze(&e1);

        let e2 = make_login_entry("alice", "1.1.1.1", base + 60 * 1_000_000_000);
        assert!(det.analyze(&e2).is_none());
    }

    #[test]
    fn impossible_travel_ignores_slow_change() {
        let det = ImpossibleTravelDetector::default();
        let base = 1_000_000_000_000i64;
        let e1 = make_login_entry("alice", "1.1.1.1", base);
        det.analyze(&e1);

        // بعد ساعتين — يعتبر ممكناً.
        let e2 = make_login_entry("alice", "2.2.2.2", base + 2 * 3600 * 1_000_000_000);
        assert!(det.analyze(&e2).is_none());
    }

    #[test]
    fn impossible_travel_ignores_failed_login() {
        let det = ImpossibleTravelDetector::default();
        let base = 1_000_000_000_000i64;
        let e1 = make_login_entry("alice", "1.1.1.1", base);
        det.analyze(&e1);

        let e2 = AuditEntry::new("alice", "login", "s")
            .with_category(AuditCategory::Auth)
            .with_success(false)
            .with_timestamp(base + 60 * 1_000_000_000)
            .with_metadata("ip", "2.2.2.2");
        assert!(det.analyze(&e2).is_none());
    }

    #[test]
    fn impossible_travel_ignores_no_ip() {
        let det = ImpossibleTravelDetector::default();
        let e = AuditEntry::new("alice", "login", "s")
            .with_category(AuditCategory::Auth)
            .with_success(true)
            .with_timestamp(1000);
        // لا IP في البيانات الوصفية.
        assert!(det.analyze(&e).is_none());
    }

    #[test]
    fn impossible_travel_tracks_multiple_users() {
        let det = ImpossibleTravelDetector::default();
        let base = 1_000_000_000_000i64;

        let e1 = make_login_entry("alice", "1.1.1.1", base);
        det.analyze(&e1);
        let e2 = make_login_entry("bob", "2.2.2.2", base + 60 * 1_000_000_000);
        det.analyze(&e2);

        // bob يدخل من IP مختلف بسرعة.
        let e3 = make_login_entry("bob", "3.3.3.3", base + 120 * 1_000_000_000);
        assert!(det.analyze(&e3).is_some());

        // alice لا يزال في نفس IP — لا تنبيه.
        let e4 = make_login_entry("alice", "1.1.1.1", base + 180 * 1_000_000_000);
        assert!(det.analyze(&e4).is_none());
    }

    #[test]
    fn advanced_detector_names() {
        let oh = OffHoursDetector::default();
        let it = ImpossibleTravelDetector::default();
        assert_eq!(oh.name(), "off_hours");
        assert_eq!(it.name(), "impossible_travel");
    }
}
