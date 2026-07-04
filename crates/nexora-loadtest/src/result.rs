//! نتائج اختبار التحمل.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// نتائج اختبار التحمل.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadTestResult {
    /// إجمالي الطلبات المرسلة.
    pub total_sent: usize,
    /// الطلبات الناجحة.
    pub successful: usize,
    /// الطلبات الفاشلة.
    pub failed: usize,
    /// إجمالي الوقت (ثوانٍ).
    pub elapsed_seconds: f64,
    /// أقل زمن استجابة (ميكروثانية).
    pub min_latency_us: u64,
    /// أكثر زمن استجابة (ميكروثانية).
    pub max_latency_us: u64,
    /// متوسط زمن الاستجابة (ميكروثانية).
    pub avg_latency_us: u64,
    /// النسبة المئوية 95 (ميكروثانية).
    pub p95_latency_us: u64,
    /// النسبة المئوية 99 (ميكروثانية).
    pub p99_latency_us: u64,
    /// الأخطاء حسب النوع.
    pub errors: std::collections::HashMap<String, usize>,
}

impl LoadTestResult {
    /// طلبات في الثانية.
    pub fn requests_per_second(&self) -> f64 {
        if self.elapsed_seconds > 0.0 {
            self.total_sent as f64 / self.elapsed_seconds
        } else {
            0.0
        }
    }

    /// معدل النجاح.
    pub fn success_rate(&self) -> f64 {
        if self.total_sent == 0 {
            return 1.0;
        }
        self.successful as f64 / self.total_sent as f64
    }

    /// معدل الخطأ.
    pub fn error_rate(&self) -> f64 {
        1.0 - self.success_rate()
    }

    /// متوسط زمن الاستجابة بالمللي ثانية.
    pub fn avg_latency_ms(&self) -> f64 {
        self.avg_latency_us as f64 / 1000.0
    }

    /// P95 بالمللي ثانية.
    pub fn p95_latency_ms(&self) -> f64 {
        self.p95_latency_us as f64 / 1000.0
    }

    /// P99 بالمللي ثانية.
    pub fn p99_latency_ms(&self) -> f64 {
        self.p99_latency_us as f64 / 1000.0
    }

    /// ملخص نصي.
    pub fn summary(&self) -> String {
        format!(
            "{} طلب | {} نجح | {} فشل | {:.0} RPS | {:.1}% خطأ | avg {:.2}ms | p95 {:.2}ms | p99 {:.2}ms | {:.2}s",
            self.total_sent,
            self.successful,
            self.failed,
            self.requests_per_second(),
            self.error_rate() * 100.0,
            self.avg_latency_ms(),
            self.p95_latency_ms(),
            self.p99_latency_ms(),
            self.elapsed_seconds,
        )
    }
}

/// منشئ النتائج من قائمة زمن الاستجابة.
pub fn build_result(
    latencies_us: Vec<u64>,
    successful: usize,
    failed: usize,
    elapsed: Duration,
    errors: std::collections::HashMap<String, usize>,
) -> LoadTestResult {
    let total_sent = successful + failed;
    let mut sorted = latencies_us.clone();
    sorted.sort_unstable();

    let min = sorted.first().copied().unwrap_or(0);
    let max = sorted.last().copied().unwrap_or(0);
    let avg = if sorted.is_empty() {
        0
    } else {
        sorted.iter().sum::<u64>() / sorted.len() as u64
    };

    let percentile = |p: f64| -> u64 {
        if sorted.is_empty() {
            return 0;
        }
        let idx = ((sorted.len() as f64 * p).ceil() as usize).saturating_sub(1);
        sorted[idx.min(sorted.len() - 1)]
    };

    LoadTestResult {
        total_sent,
        successful,
        failed,
        elapsed_seconds: elapsed.as_secs_f64(),
        min_latency_us: min,
        max_latency_us: max,
        avg_latency_us: avg,
        p95_latency_us: percentile(0.95),
        p99_latency_us: percentile(0.99),
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn rps_calculation() {
        let r = LoadTestResult {
            total_sent: 1000,
            successful: 950,
            failed: 50,
            elapsed_seconds: 10.0,
            min_latency_us: 100,
            max_latency_us: 5000,
            avg_latency_us: 500,
            p95_latency_us: 2000,
            p99_latency_us: 4000,
            errors: std::collections::HashMap::new(),
        };
        assert!((r.requests_per_second() - 100.0).abs() < 0.01);
        assert!((r.success_rate() - 0.95).abs() < 0.01);
        assert!((r.error_rate() - 0.05).abs() < 0.01);
    }

    #[test]
    fn latency_ms_conversions() {
        let r = LoadTestResult {
            total_sent: 10,
            successful: 10,
            failed: 0,
            elapsed_seconds: 1.0,
            min_latency_us: 1000,
            max_latency_us: 5000,
            avg_latency_us: 2000,
            p95_latency_us: 4000,
            p99_latency_us: 5000,
            errors: std::collections::HashMap::new(),
        };
        assert!((r.avg_latency_ms() - 2.0).abs() < 0.01);
        assert!((r.p95_latency_ms() - 4.0).abs() < 0.01);
        assert!((r.p99_latency_ms() - 5.0).abs() < 0.01);
    }

    #[test]
    fn summary_contains_stats() {
        let r = LoadTestResult {
            total_sent: 100,
            successful: 90,
            failed: 10,
            elapsed_seconds: 5.0,
            min_latency_us: 100,
            max_latency_us: 1000,
            avg_latency_us: 500,
            p95_latency_us: 800,
            p99_latency_us: 900,
            errors: std::collections::HashMap::new(),
        };
        let s = r.summary();
        assert!(s.contains("100 طلب"));
        assert!(s.contains("90 نجح"));
        assert!(s.contains("10 فشل"));
        assert!(s.contains("RPS"));
    }

    #[test]
    fn empty_result() {
        let r = LoadTestResult {
            total_sent: 0,
            successful: 0,
            failed: 0,
            elapsed_seconds: 0.0,
            min_latency_us: 0,
            max_latency_us: 0,
            avg_latency_us: 0,
            p95_latency_us: 0,
            p99_latency_us: 0,
            errors: std::collections::HashMap::new(),
        };
        assert_eq!(r.requests_per_second(), 0.0);
        assert_eq!(r.success_rate(), 1.0);
    }

    #[test]
    fn build_result_calculates_percentiles() {
        let latencies: Vec<u64> = (1..=100).collect();
        let r = build_result(
            latencies,
            100,
            0,
            Duration::from_secs(1),
            std::collections::HashMap::new(),
        );
        assert_eq!(r.total_sent, 100);
        assert_eq!(r.min_latency_us, 1);
        assert_eq!(r.max_latency_us, 100);
        // P95 of 1..=100 = 95
        assert_eq!(r.p95_latency_us, 95);
        // P99 of 1..=100 = 99
        assert_eq!(r.p99_latency_us, 99);
    }

    #[test]
    fn build_result_empty() {
        let r = build_result(
            vec![],
            0,
            0,
            Duration::from_secs(0),
            std::collections::HashMap::new(),
        );
        assert_eq!(r.total_sent, 0);
        assert_eq!(r.avg_latency_us, 0);
    }

    #[test]
    fn build_result_avg() {
        let latencies = vec![100, 200, 300, 400, 500];
        let r = build_result(latencies, 5, 0, Duration::from_secs(1), std::collections::HashMap::new());
        assert_eq!(r.avg_latency_us, 300);
    }

    #[test]
    fn serde_roundtrip() {
        let r = LoadTestResult {
            total_sent: 100,
            successful: 90,
            failed: 10,
            elapsed_seconds: 5.0,
            min_latency_us: 10,
            max_latency_us: 1000,
            avg_latency_us: 200,
            p95_latency_us: 500,
            p99_latency_us: 900,
            errors: std::collections::HashMap::new(),
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: LoadTestResult = serde_json::from_str(&json).unwrap();
        assert_eq!(r.total_sent, back.total_sent);
        assert!((r.elapsed_seconds - back.elapsed_seconds).abs() < 0.01);
    }
}
