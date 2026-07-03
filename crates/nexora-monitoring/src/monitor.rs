//! المراقب — يجمع المقاييس والصحة.

use crate::health::{HealthProbeManager, HealthStatus};
use crate::metrics::MetricsCollector;
use serde::{Deserialize, Serialize};

/// لقطة المراقبة.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorSnapshot {
    /// الحالة الإجمالية.
    pub overall_health: String,
    /// المقاييس الإجمالية.
    pub total_requests: u64,
    pub successful: u64,
    pub failed: u64,
    pub avg_latency_us: u64,
    pub error_rate: f64,
    /// عدد المسارات المتتبّعة.
    pub tracked_paths: usize,
    /// عدد فحوصات الصحة.
    pub probe_count: usize,
    /// أعلى المسارات بطئاً.
    pub slowest_paths: Vec<(String, u64)>,
    /// أكثر المسارات أخطاءً.
    pub error_paths: Vec<(String, u64)>,
}

/// المراقب.
pub struct Monitor {
    pub metrics: MetricsCollector,
    pub health: HealthProbeManager,
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Monitor {
    pub fn new() -> Self {
        Self {
            metrics: MetricsCollector::new(),
            health: HealthProbeManager::new(),
        }
    }

    /// لقطة الحالة الحالية.
    pub fn snapshot(&self) -> MonitorSnapshot {
        let g = self.metrics.global_metrics();
        MonitorSnapshot {
            overall_health: self.health.overall_status().as_str().to_string(),
            total_requests: g.total_requests,
            successful: g.successful,
            failed: g.failed,
            avg_latency_us: g.avg_latency_us(),
            error_rate: g.error_rate(),
            tracked_paths: self.metrics.path_count(),
            probe_count: self.health.probe_count(),
            slowest_paths: self.metrics.slowest_paths(5),
            error_paths: self.metrics.error_paths(5),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::SimpleProbe;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn snapshot_empty() {
        let m = Monitor::new();
        let s = m.snapshot();
        assert_eq!(s.overall_health, "healthy");
        assert_eq!(s.total_requests, 0);
    }

    #[test]
    fn snapshot_with_metrics() {
        let m = Monitor::new();
        m.metrics.record("/api/test", Duration::from_micros(100), 200);
        m.metrics.record("/api/test", Duration::from_micros(200), 500);
        let s = m.snapshot();
        assert_eq!(s.total_requests, 2);
        assert_eq!(s.successful, 1);
        assert_eq!(s.failed, 1);
        assert!((s.error_rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn snapshot_with_health() {
        let m = Monitor::new();
        m.health.register(Arc::new(SimpleProbe::new("db", || {
            crate::health::ProbeResult::unhealthy("db", "down")
        })));
        m.health.run_all();
        let s = m.snapshot();
        assert_eq!(s.overall_health, "unhealthy");
        assert_eq!(s.probe_count, 1);
    }

    #[test]
    fn snapshot_slowest_paths() {
        let m = Monitor::new();
        m.metrics.record("/fast", Duration::from_micros(10), 200);
        m.metrics.record("/slow", Duration::from_micros(500), 200);
        let s = m.snapshot();
        assert!(!s.slowest_paths.is_empty());
        assert_eq!(s.slowest_paths[0].0, "/slow");
    }

    #[test]
    fn snapshot_error_paths() {
        let m = Monitor::new();
        m.metrics.record("/ok", Duration::from_micros(1), 200);
        m.metrics.record("/err", Duration::from_micros(1), 500);
        let s = m.snapshot();
        assert!(!s.error_paths.is_empty());
        assert_eq!(s.error_paths[0].0, "/err");
    }
}
