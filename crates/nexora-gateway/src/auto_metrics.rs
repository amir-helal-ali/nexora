//! Middleware لتتبع كل طلب تلقائياً.
//!
//! يلتف حول كل طلب HTTP ويسجّل:
//! - المسار
//! - مدة المعالجة
//! - كود الحالة
//!
//! في `Monitor` تلقائياً بدون تدخل يدوي.

use crate::routes::GatewayState;
use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::time::Instant;

/// Middleware لتتبع الطلبات تلقائياً في Monitor.
pub async fn auto_metrics_middleware(
    State(state): State<GatewayState>,
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let path = req.uri().path().to_string();
    let start = Instant::now();

    let response = next.run(req).await;

    let latency = start.elapsed();
    let status_code = response.status().as_u16() as u16;
    state.monitor.metrics.record(&path, latency, status_code);

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_monitoring::Monitor;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn monitor_records_correctly() {
        let mon = Arc::new(Monitor::new());
        mon.metrics.record("/api/test", Duration::from_micros(100), 200);
        mon.metrics.record("/api/test", Duration::from_micros(200), 200);
        mon.metrics.record("/api/test", Duration::from_micros(50), 500);

        let g = mon.metrics.global_metrics();
        assert_eq!(g.total_requests, 3);
        assert_eq!(g.successful, 2);
        assert_eq!(g.failed, 1);

        let snap = mon.snapshot();
        assert_eq!(snap.total_requests, 3);
        assert!((snap.error_rate - 1.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn monitor_multiple_paths() {
        let mon = Arc::new(Monitor::new());
        mon.metrics.record("/api/a", Duration::from_micros(10), 200);
        mon.metrics.record("/api/b", Duration::from_micros(20), 200);
        mon.metrics.record("/api/a", Duration::from_micros(30), 404);

        assert_eq!(mon.metrics.path_count(), 2);

        let slowest = mon.metrics.slowest_paths(2);
        assert_eq!(slowest.len(), 2);
        // /api/a avg = 20μs, /api/b avg = 20μs — كلاهما متساوي تقريباً.
    }

    #[test]
    fn monitor_reset_clears() {
        let mon = Arc::new(Monitor::new());
        mon.metrics.record("/api/test", Duration::from_micros(100), 200);
        assert_eq!(mon.metrics.global_metrics().total_requests, 1);
        mon.metrics.reset();
        assert_eq!(mon.metrics.global_metrics().total_requests, 0);
    }

    #[test]
    fn snapshot_includes_health() {
        let mon = Arc::new(Monitor::new());
        let snap = mon.snapshot();
        assert_eq!(snap.overall_health, "healthy");
        assert_eq!(snap.probe_count, 0);
    }

    #[test]
    fn error_paths_tracked() {
        let mon = Arc::new(Monitor::new());
        mon.metrics.record("/ok", Duration::from_micros(1), 200);
        mon.metrics.record("/err", Duration::from_micros(1), 500);
        mon.metrics.record("/err", Duration::from_micros(1), 500);

        let errors = mon.metrics.error_paths(5);
        assert_eq!(errors[0].0, "/err");
        assert_eq!(errors[0].1, 2);
    }
}
