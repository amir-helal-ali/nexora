//! مقاييس الأداء.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// مقاييس طلب.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestMetrics {
    /// عدد الطلبات الإجمالي.
    pub total_requests: u64,
    /// الطلبات الناجحة (2xx).
    pub successful: u64,
    /// الطلبات الفاشلة (4xx + 5xx).
    pub failed: u64,
    /// إجمالي وقت المعالجة (ميكروثانية).
    pub total_latency_us: u64,
    /// أقل وقت (ميكروثانية).
    pub min_latency_us: u64,
    /// أكثر وقت (ميكروثانية).
    pub max_latency_us: u64,
    /// عدد الأخطاء حسب الكود.
    pub errors_by_code: HashMap<u16, u64>,
}

impl RequestMetrics {
    pub fn new() -> Self {
        Self {
            min_latency_us: u64::MAX,
            ..Default::default()
        }
    }

    /// تسجيل طلب.
    pub fn record(&mut self, latency: Duration, status_code: u16) {
        self.total_requests += 1;
        let us = latency.as_micros() as u64;
        self.total_latency_us += us;
        if us < self.min_latency_us {
            self.min_latency_us = us;
        }
        if us > self.max_latency_us {
            self.max_latency_us = us;
        }
        if status_code >= 200 && status_code < 300 {
            self.successful += 1;
        } else {
            self.failed += 1;
            *self.errors_by_code.entry(status_code).or_default() += 1;
        }
    }

    /// متوسط وقت الاستجابة (ميكروثانية).
    pub fn avg_latency_us(&self) -> u64 {
        if self.total_requests == 0 {
            return 0;
        }
        self.total_latency_us / self.total_requests
    }

    /// معدل النجاح.
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 1.0;
        }
        self.successful as f64 / self.total_requests as f64
    }

    /// معدل الخطأ.
    pub fn error_rate(&self) -> f64 {
        1.0 - self.success_rate()
    }
}

/// مؤقت لقياس المدة.
pub struct Timer {
    start: Instant,
}

impl Timer {
    pub fn start() -> Self {
        Self { start: Instant::now() }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// جامع المقاييس.
pub struct MetricsCollector {
    /// مقاييس لكل مسار.
    by_path: RwLock<HashMap<String, RequestMetrics>>,
    /// مقاييس إجمالية.
    global: RwLock<RequestMetrics>,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            by_path: RwLock::new(HashMap::new()),
            global: RwLock::new(RequestMetrics::new()),
        }
    }

    /// تسجيل طلب.
    pub fn record(&self, path: &str, latency: Duration, status_code: u16) {
        self.global.write().record(latency, status_code);
        self.by_path
            .write()
            .entry(path.to_string())
            .or_insert_with(RequestMetrics::new)
            .record(latency, status_code);
    }

    /// المقاييس الإجمالية.
    pub fn global_metrics(&self) -> RequestMetrics {
        self.global.read().clone()
    }

    /// مقاييس مسار محدد.
    pub fn path_metrics(&self, path: &str) -> Option<RequestMetrics> {
        self.by_path.read().get(path).cloned()
    }

    /// كل المسارات المتتبّعة.
    pub fn tracked_paths(&self) -> Vec<String> {
        self.by_path.read().keys().cloned().collect()
    }

    /// عدد المسارات المتتبّعة.
    pub fn path_count(&self) -> usize {
        self.by_path.read().len()
    }

    /// أعلى المسارات بطئاً.
    pub fn slowest_paths(&self, limit: usize) -> Vec<(String, u64)> {
        let paths = self.by_path.read();
        let mut sorted: Vec<(String, u64)> = paths
            .iter()
            .map(|(p, m)| (p.clone(), m.avg_latency_us()))
            .collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(limit);
        sorted
    }

    /// أكثر المسارات أخطاءً.
    pub fn error_paths(&self, limit: usize) -> Vec<(String, u64)> {
        let paths = self.by_path.read();
        let mut sorted: Vec<(String, u64)> = paths
            .iter()
            .map(|(p, m)| (p.clone(), m.failed))
            .collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(limit);
        sorted
    }

    /// إعادة ضبط كل المقاييس.
    pub fn reset(&self) {
        self.by_path.write().clear();
        *self.global.write() = RequestMetrics::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_success() {
        let mut m = RequestMetrics::new();
        m.record(Duration::from_micros(100), 200);
        m.record(Duration::from_micros(200), 200);
        assert_eq!(m.total_requests, 2);
        assert_eq!(m.successful, 2);
        assert_eq!(m.failed, 0);
        assert_eq!(m.avg_latency_us(), 150);
    }

    #[test]
    fn record_failure() {
        let mut m = RequestMetrics::new();
        m.record(Duration::from_micros(50), 200);
        m.record(Duration::from_micros(50), 500);
        m.record(Duration::from_micros(50), 404);
        assert_eq!(m.total_requests, 3);
        assert_eq!(m.successful, 1);
        assert_eq!(m.failed, 2);
        assert_eq!(m.errors_by_code.get(&500), Some(&1));
        assert_eq!(m.errors_by_code.get(&404), Some(&1));
    }

    #[test]
    fn success_rate() {
        let mut m = RequestMetrics::new();
        m.record(Duration::from_micros(10), 200);
        m.record(Duration::from_micros(10), 200);
        m.record(Duration::from_micros(10), 500);
        assert!((m.success_rate() - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn empty_metrics() {
        let m = RequestMetrics::new();
        assert_eq!(m.avg_latency_us(), 0);
        assert_eq!(m.success_rate(), 1.0);
    }

    #[test]
    fn timer_measures() {
        let t = Timer::start();
        std::thread::sleep(Duration::from_millis(10));
        assert!(t.elapsed() >= Duration::from_millis(10));
    }

    #[test]
    fn collector_record() {
        let c = MetricsCollector::new();
        c.record("/api/test", Duration::from_micros(100), 200);
        c.record("/api/test", Duration::from_micros(200), 200);
        let g = c.global_metrics();
        assert_eq!(g.total_requests, 2);
        let p = c.path_metrics("/api/test").unwrap();
        assert_eq!(p.total_requests, 2);
    }

    #[test]
    fn collector_multiple_paths() {
        let c = MetricsCollector::new();
        c.record("/api/a", Duration::from_micros(50), 200);
        c.record("/api/b", Duration::from_micros(200), 200);
        c.record("/api/a", Duration::from_micros(100), 500);
        assert_eq!(c.path_count(), 2);
    }

    #[test]
    fn collector_slowest_paths() {
        let c = MetricsCollector::new();
        c.record("/fast", Duration::from_micros(10), 200);
        c.record("/slow", Duration::from_micros(500), 200);
        c.record("/medium", Duration::from_micros(100), 200);
        let slowest = c.slowest_paths(2);
        assert_eq!(slowest[0].0, "/slow");
        assert_eq!(slowest.len(), 2);
    }

    #[test]
    fn collector_error_paths() {
        let c = MetricsCollector::new();
        c.record("/ok", Duration::from_micros(10), 200);
        c.record("/err1", Duration::from_micros(10), 500);
        c.record("/err1", Duration::from_micros(10), 500);
        c.record("/err2", Duration::from_micros(10), 404);
        let errors = c.error_paths(2);
        assert_eq!(errors[0].0, "/err1");
        assert_eq!(errors[0].1, 2);
    }

    #[test]
    fn collector_reset() {
        let c = MetricsCollector::new();
        c.record("/test", Duration::from_micros(10), 200);
        assert_eq!(c.global_metrics().total_requests, 1);
        c.reset();
        assert_eq!(c.global_metrics().total_requests, 0);
        assert_eq!(c.path_count(), 0);
    }

    #[test]
    fn min_max_latency() {
        let mut m = RequestMetrics::new();
        m.record(Duration::from_micros(50), 200);
        m.record(Duration::from_micros(200), 200);
        m.record(Duration::from_micros(100), 200);
        assert_eq!(m.min_latency_us, 50);
        assert_eq!(m.max_latency_us, 200);
    }

    #[test]
    fn tracked_paths() {
        let c = MetricsCollector::new();
        c.record("/a", Duration::from_micros(1), 200);
        c.record("/b", Duration::from_micros(1), 200);
        let paths = c.tracked_paths();
        assert!(paths.contains(&"/a".to_string()));
        assert!(paths.contains(&"/b".to_string()));
    }
}
