//! تصدير مقاييس بصيغة Prometheus.
//!
//! يحوّل مقاييس `MetricsCollector` إلى نص بصيغة Prometheus exposition:
//!
//! ```text
//! # HELP nexora_requests_total Total number of HTTP requests
//! # TYPE nexora_requests_total counter
//! nexora_requests_total 12345
//! nexora_requests_total{status="success"} 10000
//! nexora_requests_total{status="error"} 2345
//! ```

use crate::metrics::MetricsCollector;

/// تصدير كل المقاييس بصيغة Prometheus.
pub fn export_prometheus(collector: &MetricsCollector) -> String {
    let global = collector.global_metrics();
    let mut out = String::with_capacity(2048);

    // عدّاد الطلبات الإجمالي.
    out.push_str("# HELP nexora_requests_total Total HTTP requests\n");
    out.push_str("# TYPE nexora_requests_total counter\n");
    out.push_str(&format!("nexora_requests_total {}\n", global.total_requests));
    out.push_str(&format!(
        "nexora_requests_total{{status=\"success\"}} {}\n",
        global.successful
    ));
    out.push_str(&format!(
        "nexora_requests_total{{status=\"error\"}} {}\n",
        global.failed
    ));

    // زمن الاستجابة.
    out.push_str("\n# HELP nexora_latency_microseconds Request latency in microseconds\n");
    out.push_str("# TYPE nexora_latency_microseconds summary\n");
    out.push_str(&format!(
        "nexora_latency_microseconds{{quantile=\"avg\"}} {}\n",
        global.avg_latency_us()
    ));
    out.push_str(&format!(
        "nexora_latency_microseconds{{quantile=\"min\"}} {}\n",
        if global.min_latency_us == u64::MAX { 0 } else { global.min_latency_us }
    ));
    out.push_str(&format!(
        "nexora_latency_microseconds{{quantile=\"max\"}} {}\n",
        global.max_latency_us
    ));
    out.push_str(&format!(
        "nexora_latency_microseconds_sum {}\n",
        global.total_latency_us
    ));

    // معدل الخطأ.
    out.push_str("\n# HELP nexora_error_rate Current error rate (0.0-1.0)\n");
    out.push_str("# TYPE nexora_error_rate gauge\n");
    out.push_str(&format!("nexora_error_rate {}\n", global.error_rate()));

    // معدل النجاح.
    out.push_str("\n# HELP nexora_success_rate Current success rate (0.0-1.0)\n");
    out.push_str("# TYPE nexora_success_rate gauge\n");
    out.push_str(&format!("nexora_success_rate {}\n", global.success_rate()));

    // الأخطاء حسب الكود.
    if !global.errors_by_code.is_empty() {
        out.push_str("\n# HELP nexora_errors_by_code Errors grouped by HTTP status code\n");
        out.push_str("# TYPE nexora_errors_by_code counter\n");
        let mut codes: Vec<_> = global.errors_by_code.iter().collect();
        codes.sort_by_key(|(k, _)| **k);
        for (code, count) in &codes {
            out.push_str(&format!(
                "nexora_errors_by_code{{code=\"{code}\"}} {count}\n"
            ));
        }
    }

    // المسارات المتتبّعة.
    out.push_str("\n# HELP nexora_tracked_paths Number of tracked API paths\n");
    out.push_str("# TYPE nexora_tracked_paths gauge\n");
    out.push_str(&format!("nexora_tracked_paths {}\n", collector.path_count()));

    // مقاييس لكل مسار.
    let paths = collector.tracked_paths();
    if !paths.is_empty() {
        out.push_str("\n# HELP nexora_path_requests Requests per path\n");
        out.push_str("# TYPE nexora_path_requests counter\n");
        for p in &paths {
            if let Some(m) = collector.path_metrics(p) {
                let safe_path = sanitize_label(p);
                out.push_str(&format!(
                    "nexora_path_requests{{path=\"{safe_path}\"}} {}\n",
                    m.total_requests
                ));
            }
        }

        out.push_str("\n# HELP nexora_path_latency_avg Average latency per path (microseconds)\n");
        out.push_str("# TYPE nexora_path_latency_avg gauge\n");
        for p in &paths {
            if let Some(m) = collector.path_metrics(p) {
                let safe_path = sanitize_label(p);
                out.push_str(&format!(
                    "nexora_path_latency_avg{{path=\"{safe_path}\"}} {}\n",
                    m.avg_latency_us()
                ));
            }
        }

        // path_errors: أضف الترويسة فقط عند وجود أخطاء.
        let has_errors = paths.iter().any(|p| {
            collector.path_metrics(p).map(|m| m.failed > 0).unwrap_or(false)
        });
        if has_errors {
            out.push_str("\n# HELP nexora_path_errors Failed requests per path\n");
            out.push_str("# TYPE nexora_path_errors counter\n");
            for p in &paths {
                if let Some(m) = collector.path_metrics(p) {
                    if m.failed > 0 {
                        let safe_path = sanitize_label(p);
                        out.push_str(&format!(
                            "nexora_path_errors{{path=\"{safe_path}\"}} {}\n",
                            m.failed
                        ));
                    }
                }
            }
        }
    }

    out
}

/// تنظيف قيمة label (إزالة الأحرف الخاصة).
fn sanitize_label(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn empty_export_has_headers() {
        let c = MetricsCollector::new();
        let out = export_prometheus(&c);
        assert!(out.contains("# HELP nexora_requests_total"));
        assert!(out.contains("# TYPE nexora_requests_total counter"));
        assert!(out.contains("nexora_requests_total 0"));
    }

    #[test]
    fn export_includes_success_and_error() {
        let c = MetricsCollector::new();
        c.record("/api/test", Duration::from_micros(100), 200);
        c.record("/api/test", Duration::from_micros(200), 500);
        let out = export_prometheus(&c);
        assert!(out.contains(r#"status="success""#));
        assert!(out.contains(r#"status="error""#));
    }

    #[test]
    fn export_includes_latency() {
        let c = MetricsCollector::new();
        c.record("/api/test", Duration::from_micros(100), 200);
        let out = export_prometheus(&c);
        assert!(out.contains("nexora_latency_microseconds"));
        assert!(out.contains("quantile=\"avg\""));
        assert!(out.contains("quantile=\"min\""));
        assert!(out.contains("quantile=\"max\""));
    }

    #[test]
    fn export_includes_error_rate() {
        let c = MetricsCollector::new();
        c.record("/api/test", Duration::from_micros(1), 200);
        c.record("/api/test", Duration::from_micros(1), 500);
        let out = export_prometheus(&c);
        assert!(out.contains("nexora_error_rate"));
        assert!(out.contains("nexora_success_rate"));
    }

    #[test]
    fn export_includes_errors_by_code() {
        let c = MetricsCollector::new();
        c.record("/api/test", Duration::from_micros(1), 500);
        c.record("/api/test", Duration::from_micros(1), 404);
        let out = export_prometheus(&c);
        assert!(out.contains("nexora_errors_by_code"));
        assert!(out.contains(r#"code="500""#));
        assert!(out.contains(r#"code="404""#));
    }

    #[test]
    fn export_includes_tracked_paths() {
        let c = MetricsCollector::new();
        c.record("/api/a", Duration::from_micros(1), 200);
        c.record("/api/b", Duration::from_micros(1), 200);
        let out = export_prometheus(&c);
        assert!(out.contains("nexora_tracked_paths 2"));
        assert!(out.contains("nexora_path_requests"));
        assert!(out.contains(r#"path="/api/a""#));
        assert!(out.contains(r#"path="/api/b""#));
    }

    #[test]
    fn export_includes_path_latency() {
        let c = MetricsCollector::new();
        c.record("/api/slow", Duration::from_micros(500), 200);
        let out = export_prometheus(&c);
        assert!(out.contains("nexora_path_latency_avg"));
        assert!(out.contains(r#"path="/api/slow""#));
    }

    #[test]
    fn export_includes_path_errors() {
        let c = MetricsCollector::new();
        c.record("/api/err", Duration::from_micros(1), 500);
        c.record("/api/ok", Duration::from_micros(1), 200);
        let out = export_prometheus(&c);
        assert!(out.contains("nexora_path_errors"));
        assert!(out.contains(r#"path="/api/err""#));
        // /api/ok لا أخطاء → لا يجب أن يظهر في path_errors.
        let lines: Vec<&str> = out.lines().collect();
        let err_lines: Vec<&&str> = lines
            .iter()
            .filter(|l| l.starts_with("nexora_path_errors"))
            .collect();
        assert_eq!(err_lines.len(), 1);
    }

    #[test]
    fn sanitize_escapes_quotes() {
        assert_eq!(sanitize_label(r#"a"b"#), r#"a\"b"#);
        assert_eq!(sanitize_label("a\\b"), r#"a\\b"#);
    }

    #[test]
    fn export_no_errors_by_code_when_empty() {
        let c = MetricsCollector::new();
        c.record("/api/test", Duration::from_micros(1), 200);
        let out = export_prometheus(&c);
        assert!(!out.contains("nexora_errors_by_code"));
    }

    #[test]
    fn export_path_errors_omitted_when_no_errors() {
        let c = MetricsCollector::new();
        c.record("/api/ok", Duration::from_micros(1), 200);
        let out = export_prometheus(&c);
        assert!(!out.contains("nexora_path_errors"));
    }
}
