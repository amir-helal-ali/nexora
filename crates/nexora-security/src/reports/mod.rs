//! # مولّد التقارير الأمنية
//!
//! يولّد تقارير دورية (يومية/أسبوعية) تلخص النشاط الأمني:
//! - عدد التنبيهات حسب النوع والخطورة
//! - أكثر الفاعلين نشاطاً
//! - أكثر الإجراءات الفاشلة
//! - الإحصائيات الزمنية

use crate::alert::{AlertStatus, SecurityAlert, Severity};
use crate::threat::ThreatType;
use nexora_audit::{AuditCategory, AuditEntry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use time::OffsetDateTime;

/// فترة التقرير.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportPeriod {
    Daily,
    Weekly,
    Monthly,
}

impl ReportPeriod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }

    pub fn duration_seconds(self) -> i64 {
        match self {
            Self::Daily => 86400,
            Self::Weekly => 7 * 86400,
            Self::Monthly => 30 * 86400,
        }
    }
}

/// تقرير أمني.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReport {
    /// فترة التقرير.
    pub period: ReportPeriod,
    /// بداية الفترة (unix nanos).
    pub from: i64,
    /// نهاية الفترة (unix nanos).
    pub to: i64,
    /// إجمالي التنبيهات.
    pub total_alerts: usize,
    /// تنبيهات حسب الخطورة.
    pub alerts_by_severity: HashMap<String, usize>,
    /// تنبيهات حسب النوع.
    pub alerts_by_type: HashMap<String, usize>,
    /// تنبيهات حسب الحالة.
    pub alerts_by_status: HashMap<String, usize>,
    /// أكثر الفاعلين تنبيهاً.
    pub top_alerted_actors: Vec<(String, usize)>,
    /// إجمالي مدخلات التدقيق.
    pub total_audit_entries: usize,
    /// مدخلات ناجحة.
    pub successful_entries: usize,
    /// مدخلات فاشلة.
    pub failed_entries: usize,
    /// مدخلات حسب الفئة.
    pub entries_by_category: HashMap<String, usize>,
    /// أكثر الإجراءات.
    pub top_actions: Vec<(String, usize)>,
    /// أكثر الفاعلين نشاطاً.
    pub top_actors: Vec<(String, usize)>,
    /// ملخص نصي.
    pub summary: String,
}

/// مولّد التقارير.
pub struct ReportGenerator;

impl ReportGenerator {
    /// توليد تقرير من قوائم التنبيهات والمدخلات.
    pub fn generate(
        period: ReportPeriod,
        alerts: &[SecurityAlert],
        audit_entries: &[AuditEntry],
    ) -> SecurityReport {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let from = now - period.duration_seconds() * 1_000_000_000;

        // فلترة حسب الوقت.
        let recent_alerts: Vec<&SecurityAlert> = alerts.iter().filter(|a| a.created_at >= from).collect();
        let recent_entries: Vec<&AuditEntry> = audit_entries.iter().filter(|e| e.timestamp >= from).collect();

        // تجميع التنبيهات.
        let mut alerts_by_severity: HashMap<String, usize> = HashMap::new();
        let mut alerts_by_type: HashMap<String, usize> = HashMap::new();
        let mut alerts_by_status: HashMap<String, usize> = HashMap::new();
        let mut actor_alert_counts: HashMap<String, usize> = HashMap::new();

        for a in &recent_alerts {
            *alerts_by_severity.entry(a.severity.as_str().to_string()).or_default() += 1;
            *alerts_by_type.entry(a.threat_type.as_str().to_string()).or_default() += 1;
            let status_str = match a.status {
                AlertStatus::Active => "active",
                AlertStatus::Investigating => "investigating",
                AlertStatus::Resolved => "resolved",
                AlertStatus::Dismissed => "dismissed",
            };
            *alerts_by_status.entry(status_str.to_string()).or_default() += 1;
            *actor_alert_counts.entry(a.actor.clone()).or_default() += 1;
        }

        // تجميع المدخلات.
        let mut entries_by_category: HashMap<String, usize> = HashMap::new();
        let mut action_counts: HashMap<String, usize> = HashMap::new();
        let mut actor_counts: HashMap<String, usize> = HashMap::new();
        let mut successful = 0;
        let mut failed = 0;

        for e in &recent_entries {
            *entries_by_category.entry(e.category.as_str().to_string()).or_default() += 1;
            *action_counts.entry(e.action.clone()).or_default() += 1;
            *actor_counts.entry(e.actor.clone()).or_default() += 1;
            if e.success {
                successful += 1;
            } else {
                failed += 1;
            }
        }

        // ترتيب القوائم.
        let mut top_alerted_actors: Vec<(String, usize)> = actor_alert_counts.into_iter().collect();
        top_alerted_actors.sort_by(|a, b| b.1.cmp(&a.1));
        top_alerted_actors.truncate(10);

        let mut top_actions: Vec<(String, usize)> = action_counts.into_iter().collect();
        top_actions.sort_by(|a, b| b.1.cmp(&a.1));
        top_actions.truncate(10);

        let mut top_actors: Vec<(String, usize)> = actor_counts.into_iter().collect();
        top_actors.sort_by(|a, b| b.1.cmp(&a.1));
        top_actors.truncate(10);

        let summary = Self::build_summary(
            period,
            recent_alerts.len(),
            recent_entries.len(),
            failed,
        );

        SecurityReport {
            period,
            from,
            to: now,
            total_alerts: recent_alerts.len(),
            alerts_by_severity,
            alerts_by_type,
            alerts_by_status,
            top_alerted_actors,
            total_audit_entries: recent_entries.len(),
            successful_entries: successful,
            failed_entries: failed,
            entries_by_category,
            top_actions,
            top_actors,
            summary,
        }
    }

    fn build_summary(period: ReportPeriod, alerts: usize, entries: usize, failures: usize) -> String {
        let period_name = match period {
            ReportPeriod::Daily => "يومي",
            ReportPeriod::Weekly => "أسبوعي",
            ReportPeriod::Monthly => "شهري",
        };
        format!(
            "تقرير {period_name}: {alerts} تنبيه أمني، {entries} نشاط، {failures} فشل. \
             نسبة الفشل: {:.1}%",
            if entries > 0 { failures as f64 / entries as f64 * 100.0 } else { 0.0 }
        )
    }
}

/// تجميع التنبيهات حسب الحالة.
pub fn count_alerts_by_status(alerts: &[SecurityAlert]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for a in alerts {
        let status_str = match a.status {
            AlertStatus::Active => "active",
            AlertStatus::Investigating => "investigating",
            AlertStatus::Resolved => "resolved",
            AlertStatus::Dismissed => "dismissed",
        };
        *counts.entry(status_str.to_string()).or_default() += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threat::ThreatIndicator;
    use nexora_audit::AuditEntry;

    fn make_alert(actor: &str, severity: Severity, threat: ThreatType, ts: i64) -> SecurityAlert {
        let mut a = SecurityAlert::new(
            actor,
            severity,
            threat,
            "test",
            vec![ThreatIndicator::new(threat, "test", "x", 0.5)],
        );
        a.created_at = ts;
        a
    }

    fn make_entry(actor: &str, action: &str, success: bool, ts: i64) -> AuditEntry {
        AuditEntry::new(actor, action, "target")
            .with_category(AuditCategory::Auth)
            .with_success(success)
            .with_timestamp(ts)
    }

    #[test]
    fn report_period_as_str() {
        assert_eq!(ReportPeriod::Daily.as_str(), "daily");
        assert_eq!(ReportPeriod::Weekly.as_str(), "weekly");
        assert_eq!(ReportPeriod::Monthly.as_str(), "monthly");
    }

    #[test]
    fn report_period_duration() {
        assert_eq!(ReportPeriod::Daily.duration_seconds(), 86400);
        assert_eq!(ReportPeriod::Weekly.duration_seconds(), 604800);
    }

    #[test]
    fn generate_empty_report() {
        let report = ReportGenerator::generate(
            ReportPeriod::Daily,
            &[],
            &[],
        );
        assert_eq!(report.total_alerts, 0);
        assert_eq!(report.total_audit_entries, 0);
    }

    #[test]
    fn generate_with_alerts() {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let alerts = vec![
            make_alert("alice", Severity::High, ThreatType::BruteForce, now),
            make_alert("bob", Severity::Critical, ThreatType::SuspiciousIp, now),
        ];
        let report = ReportGenerator::generate(ReportPeriod::Daily, &alerts, &[]);
        assert_eq!(report.total_alerts, 2);
        assert_eq!(report.alerts_by_severity.get("high"), Some(&1));
        assert_eq!(report.alerts_by_severity.get("critical"), Some(&1));
    }

    #[test]
    fn generate_with_entries() {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let entries = vec![
            make_entry("alice", "login", true, now),
            make_entry("bob", "login", false, now),
            make_entry("alice", "read", true, now),
        ];
        let report = ReportGenerator::generate(ReportPeriod::Daily, &[], &entries);
        assert_eq!(report.total_audit_entries, 3);
        assert_eq!(report.successful_entries, 2);
        assert_eq!(report.failed_entries, 1);
    }

    #[test]
    fn report_top_actors() {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let entries = vec![
            make_entry("alice", "a", true, now),
            make_entry("alice", "b", true, now),
            make_entry("alice", "c", true, now),
            make_entry("bob", "d", true, now),
        ];
        let report = ReportGenerator::generate(ReportPeriod::Daily, &[], &entries);
        assert!(!report.top_actors.is_empty());
        assert_eq!(report.top_actors[0].0, "alice");
        assert_eq!(report.top_actors[0].1, 3);
    }

    #[test]
    fn report_summary_contains_stats() {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let alerts = vec![make_alert("a", Severity::Low, ThreatType::Other, now)];
        let entries = vec![
            make_entry("a", "x", true, now),
            make_entry("a", "y", false, now),
        ];
        let report = ReportGenerator::generate(ReportPeriod::Weekly, &alerts, &entries);
        assert!(report.summary.contains("أسبوعي"));
        assert!(report.summary.contains("1 تنبيه"));
        assert!(report.summary.contains("50.0%"));
    }

    #[test]
    fn report_filters_old_entries() {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let old_ts = now - 2 * 86400 * 1_000_000_000; // قبل يومين.
        let entries = vec![
            make_entry("a", "recent", true, now),
            make_entry("a", "old", true, old_ts),
        ];
        let report = ReportGenerator::generate(ReportPeriod::Daily, &[], &entries);
        assert_eq!(report.total_audit_entries, 1); // فقط الحديث.
    }

    #[test]
    fn count_alerts_by_status() {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let mut a1 = make_alert("a", Severity::Low, ThreatType::Other, now);
        a1.resolve();
        let a2 = make_alert("b", Severity::Low, ThreatType::Other, now);

        let counts = super::count_alerts_by_status(&[a1, a2]);
        assert_eq!(counts.get("resolved"), Some(&1));
        assert_eq!(counts.get("active"), Some(&1));
    }

    #[test]
    fn report_entries_by_category() {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let entries = vec![
            AuditEntry::new("a", "login", "s")
                .with_category(AuditCategory::Auth)
                .with_timestamp(now),
            AuditEntry::new("a", "pay", "s")
                .with_category(AuditCategory::Billing)
                .with_timestamp(now),
            AuditEntry::new("a", "login", "s")
                .with_category(AuditCategory::Auth)
                .with_timestamp(now),
        ];
        let report = ReportGenerator::generate(ReportPeriod::Daily, &[], &entries);
        assert_eq!(report.entries_by_category.get("auth"), Some(&2));
        assert_eq!(report.entries_by_category.get("billing"), Some(&1));
    }

    #[test]
    fn serde_roundtrip() {
        let report = ReportGenerator::generate(ReportPeriod::Daily, &[], &[]);
        let json = serde_json::to_string(&report).unwrap();
        let back: SecurityReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report.period, back.period);
        assert_eq!(report.total_alerts, back.total_alerts);
    }
}
