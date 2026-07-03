//! تقارير مجدولة — توليد تلقائي للتقارير الأمنية بشكل دوري.
//!
//! يدعم:
//! - تقارير يومية (كل 24 ساعة)
//! - تقارير أسبوعية (كل 7 أيام)
//! - إرسال التقارير عبر الإشعارات

use crate::alerts::PerformanceAlerter;
use crate::metrics::MetricsCollector;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;

/// فترة الجدولة.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulePeriod {
    Hourly,
    Daily,
    Weekly,
}

impl SchedulePeriod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }

    pub fn interval_seconds(self) -> u64 {
        match self {
            Self::Hourly => 3600,
            Self::Daily => 86400,
            Self::Weekly => 7 * 86400,
        }
    }
}

/// تقرير مجدول مُولّد.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledReport {
    /// معرّف فريد.
    pub id: String,
    /// الفترة.
    pub period: SchedulePeriod,
    /// وقت التوليد (unix nanos).
    pub generated_at: i64,
    /// إجمالي الطلبات.
    pub total_requests: u64,
    /// طلبات ناجحة.
    pub successful: u64,
    /// طلبات فاشلة.
    pub failed: u64,
    /// متوسط زمن الاستجابة (μs).
    pub avg_latency_us: u64,
    /// معدل الخطأ.
    pub error_rate: f64,
    /// عدد المسارات المتتبّعة.
    pub tracked_paths: usize,
    /// عدد قواعد التنبيه.
    pub alert_rules: usize,
    /// عدد التنبيهات الأخيرة.
    pub recent_alerts: usize,
    /// ملخص نصي.
    pub summary: String,
}

impl ScheduledReport {
    /// توليد تقرير من Monitor + Alerter.
    pub fn generate(
        period: SchedulePeriod,
        collector: &MetricsCollector,
        alerter: &PerformanceAlerter,
    ) -> Self {
        let metrics = collector.global_metrics();
        let rules = alerter.list_rules();
        let recent = alerter.recent_alerts();

        let summary = format!(
            "تقرير {}: {} طلب، {} نجاح، {} فشل ({:.1}% خطأ)، avg {}μs، {} مسار، {} تنبيه",
            period.as_str(),
            metrics.total_requests,
            metrics.successful,
            metrics.failed,
            metrics.error_rate() * 100.0,
            metrics.avg_latency_us(),
            collector.path_count(),
            recent.len(),
        );

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            period,
            generated_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            total_requests: metrics.total_requests,
            successful: metrics.successful,
            failed: metrics.failed,
            avg_latency_us: metrics.avg_latency_us(),
            error_rate: metrics.error_rate(),
            tracked_paths: collector.path_count(),
            alert_rules: rules.len(),
            recent_alerts: recent.len(),
            summary,
        }
    }
}

/// مدير التقارير المجدولة.
pub struct ReportScheduler {
    /// آخر التقارير المُولّدة (محفوظة في الذاكرة).
    reports: RwLock<Vec<ScheduledReport>>,
    /// آخر مرة تم فيها توليد كل فترة.
    last_generated: RwLock<HashMap<SchedulePeriod, i64>>,
    /// حد أقصى للتقارير المحفوظة.
    max_reports: usize,
}

use std::collections::HashMap;

impl Default for ReportScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl ReportScheduler {
    pub fn new() -> Self {
        Self {
            reports: RwLock::new(Vec::new()),
            last_generated: RwLock::new(HashMap::new()),
            max_reports: 50,
        }
    }

    /// توليد تقرير لفترة محددة (إن حان وقته).
    pub fn maybe_generate(
        &self,
        period: SchedulePeriod,
        collector: &MetricsCollector,
        alerter: &PerformanceAlerter,
    ) -> Option<ScheduledReport> {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let interval_nanos = (period.interval_seconds() as i64) * 1_000_000_000;

        let mut last_gen = self.last_generated.write();
        let last = last_gen.get(&period).copied().unwrap_or(0);

        if now - last < interval_nanos {
            return None; // لم يحن الوقت بعد.
        }

        let report = ScheduledReport::generate(period, collector, alerter);
        last_gen.insert(period, now);

        let mut reports = self.reports.write();
        reports.push(report.clone());
        if reports.len() > self.max_reports {
            reports.remove(0);
        }

        Some(report)
    }

    /// توليد تقرير فوراً (بغض النظر عن الجدولة).
    pub fn generate_now(
        &self,
        period: SchedulePeriod,
        collector: &MetricsCollector,
        alerter: &PerformanceAlerter,
    ) -> ScheduledReport {
        let report = ScheduledReport::generate(period, collector, alerter);
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        self.last_generated.write().insert(period, now);

        let mut reports = self.reports.write();
        reports.push(report.clone());
        if reports.len() > self.max_reports {
            reports.remove(0);
        }

        report
    }

    /// آخر التقارير المُولّدة.
    pub fn list_reports(&self) -> Vec<ScheduledReport> {
        self.reports.read().clone()
    }

    /// آخر تقرير لفترة محددة.
    pub fn latest_for(&self, period: SchedulePeriod) -> Option<ScheduledReport> {
        self.reports
            .read()
            .iter()
            .rev()
            .find(|r| r.period == period)
            .cloned()
    }

    /// عدد التقارير.
    pub fn count(&self) -> usize {
        self.reports.read().len()
    }

    /// مسح كل التقارير.
    pub fn clear(&self) {
        self.reports.write().clear();
        self.last_generated.write().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn schedule_period_interval() {
        assert_eq!(SchedulePeriod::Hourly.interval_seconds(), 3600);
        assert_eq!(SchedulePeriod::Daily.interval_seconds(), 86400);
        assert_eq!(SchedulePeriod::Weekly.interval_seconds(), 604800);
    }

    #[test]
    fn report_generate() {
        let collector = MetricsCollector::new();
        collector.record("/api/test", Duration::from_micros(100), 200);
        collector.record("/api/test", Duration::from_micros(200), 500);
        let alerter = PerformanceAlerter::new().with_defaults();

        let report = ScheduledReport::generate(SchedulePeriod::Daily, &collector, &alerter);
        assert_eq!(report.total_requests, 2);
        assert_eq!(report.successful, 1);
        assert_eq!(report.failed, 1);
        assert!(report.summary.contains("daily"));
        assert!(report.summary.contains("2 طلب"));
    }

    #[test]
    fn scheduler_generate_now() {
        let scheduler = ReportScheduler::new();
        let collector = MetricsCollector::new();
        let alerter = PerformanceAlerter::new().with_defaults();

        let report = scheduler.generate_now(SchedulePeriod::Daily, &collector, &alerter);
        assert_eq!(scheduler.count(), 1);
        assert_eq!(report.period, SchedulePeriod::Daily);
    }

    #[test]
    fn scheduler_maybe_generate_first_time() {
        let scheduler = ReportScheduler::new();
        let collector = MetricsCollector::new();
        let alerter = PerformanceAlerter::new().with_defaults();

        // أول مرة يجب أن يولّد.
        let report = scheduler.maybe_generate(SchedulePeriod::Hourly, &collector, &alerter);
        assert!(report.is_some());
    }

    #[test]
    fn scheduler_maybe_generate_respects_interval() {
        let scheduler = ReportScheduler::new();
        let collector = MetricsCollector::new();
        let alerter = PerformanceAlerter::new().with_defaults();

        // أول مرة.
        scheduler.maybe_generate(SchedulePeriod::Hourly, &collector, &alerter);
        // ثاني مرة فوراً — يجب ألا يولّد (ضمن الفاصل).
        let report2 = scheduler.maybe_generate(SchedulePeriod::Hourly, &collector, &alerter);
        assert!(report2.is_none());
    }

    #[test]
    fn scheduler_list_reports() {
        let scheduler = ReportScheduler::new();
        let collector = MetricsCollector::new();
        let alerter = PerformanceAlerter::new().with_defaults();

        scheduler.generate_now(SchedulePeriod::Daily, &collector, &alerter);
        scheduler.generate_now(SchedulePeriod::Weekly, &collector, &alerter);

        let reports = scheduler.list_reports();
        assert_eq!(reports.len(), 2);
    }

    #[test]
    fn scheduler_latest_for() {
        let scheduler = ReportScheduler::new();
        let collector = MetricsCollector::new();
        let alerter = PerformanceAlerter::new().with_defaults();

        scheduler.generate_now(SchedulePeriod::Daily, &collector, &alerter);
        scheduler.generate_now(SchedulePeriod::Weekly, &collector, &alerter);
        scheduler.generate_now(SchedulePeriod::Daily, &collector, &alerter);

        let latest_daily = scheduler.latest_for(SchedulePeriod::Daily);
        assert!(latest_daily.is_some());
        assert_eq!(latest_daily.unwrap().period, SchedulePeriod::Daily);
    }

    #[test]
    fn scheduler_clear() {
        let scheduler = ReportScheduler::new();
        let collector = MetricsCollector::new();
        let alerter = PerformanceAlerter::new().with_defaults();

        scheduler.generate_now(SchedulePeriod::Daily, &collector, &alerter);
        assert_eq!(scheduler.count(), 1);
        scheduler.clear();
        assert_eq!(scheduler.count(), 0);
    }

    #[test]
    fn scheduler_max_reports_eviction() {
        let scheduler = ReportScheduler::new();
        let collector = MetricsCollector::new();
        let alerter = PerformanceAlerter::new().with_defaults();

        for _ in 0..60 {
            scheduler.generate_now(SchedulePeriod::Hourly, &collector, &alerter);
        }
        assert!(scheduler.count() <= 50);
    }

    #[test]
    fn report_includes_alert_info() {
        let collector = MetricsCollector::new();
        let alerter = PerformanceAlerter::new().with_defaults();
        // ولّد تنبيهاً.
        for _ in 0..20 {
            collector.record("/api/test", Duration::from_micros(1), 500);
        }
        alerter.check(&collector);

        let report = ScheduledReport::generate(SchedulePeriod::Daily, &collector, &alerter);
        assert!(report.alert_rules >= 4);
        assert!(report.recent_alerts > 0);
    }

    #[test]
    fn serde_roundtrip() {
        let collector = MetricsCollector::new();
        collector.record("/api/test", Duration::from_micros(100), 200);
        let alerter = PerformanceAlerter::new().with_defaults();

        let report = ScheduledReport::generate(SchedulePeriod::Weekly, &collector, &alerter);
        let json = serde_json::to_string(&report).unwrap();
        let back: ScheduledReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report.id, back.id);
        assert_eq!(report.period, back.period);
        assert_eq!(report.total_requests, back.total_requests);
    }

    #[test]
    fn different_periods_independent() {
        let scheduler = ReportScheduler::new();
        let collector = MetricsCollector::new();
        let alerter = PerformanceAlerter::new().with_defaults();

        scheduler.generate_now(SchedulePeriod::Daily, &collector, &alerter);
        // Weekly مستقل — يجب أن يولّد.
        let weekly = scheduler.maybe_generate(SchedulePeriod::Weekly, &collector, &alerter);
        assert!(weekly.is_some());
    }
}
