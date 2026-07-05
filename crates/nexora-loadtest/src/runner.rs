//! مشغّل اختبار التحمل.

use crate::config::LoadTestConfig;
use crate::result::{build_result, LoadTestResult};
use std::time::Instant;

/// مشغّل اختبار التحمل.
pub struct LoadTest;

impl LoadTest {
    /// تشغيل اختبار التحمل.
    ///
    /// ملاحظة: في التنفيذ المرجعي، لا يرسل طلبات HTTP فعلية.
    /// بل يحاكي النتائج لاختبار البنية والمنطق.
    /// في الإنتاج، سيستخدم reqwest أو hyper لإرسال طلبات حقيقية.
    pub async fn run(config: LoadTestConfig) -> LoadTestResult {
        let start = Instant::now();

        let _per_worker = config.requests_per_worker();
        let mut latencies: Vec<u64> = Vec::with_capacity(config.total);
        let mut successful = 0;
        let mut failed = 0;
        let mut errors: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        // محاكاة الطلبات (في الإنتاج: إرسال HTTP حقيقي).
        for i in 0..config.total {
            // محاكاة زمن استجابة متغير.
            let latency_us = 100 + (i % 10) * 50 + (i % 100) * 10;

            // محاكاة نسبة فشل صغيرة.
            if i % 20 == 0 && i > 0 {
                failed += 1;
                *errors.entry("timeout".into()).or_default() += 1;
            } else {
                successful += 1;
                latencies.push(latency_us as u64);
            }
        }

        let elapsed = start.elapsed();

        build_result(latencies, successful, failed, elapsed, errors)
    }

    /// تشغيل اختبار متدرّج (ramp-up).
    pub async fn run_ramp_up(
        url: &str,
        start_concurrent: usize,
        end_concurrent: usize,
        steps: usize,
    ) -> Vec<LoadTestResult> {
        let mut results = Vec::new();
        let step_size = if end_concurrent > start_concurrent {
            (end_concurrent - start_concurrent) / steps.max(1)
        } else {
            1
        };

        for step in 0..steps {
            let concurrent = start_concurrent + step * step_size;
            let config = LoadTestConfig::new(url)
                .with_concurrent(concurrent)
                .with_total(concurrent * 10);
            let result = Self::run(config).await;
            results.push(result);
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_basic_test() {
        let config = LoadTestConfig::new("http://test")
            .with_concurrent(10)
            .with_total(100);
        let result = LoadTest::run(config).await;
        assert!(result.total_sent > 0);
        assert!(result.successful > 0);
        assert!(result.elapsed_seconds >= 0.0);
    }

    #[tokio::test]
    async fn run_result_has_latency() {
        let config = LoadTestConfig::new("http://test").with_total(50);
        let result = LoadTest::run(config).await;
        assert!(result.avg_latency_us > 0);
        assert!(result.max_latency_us >= result.min_latency_us);
    }

    #[tokio::test]
    async fn run_result_has_percentiles() {
        let config = LoadTestConfig::new("http://test").with_total(100);
        let result = LoadTest::run(config).await;
        assert!(result.p95_latency_us > 0);
        assert!(result.p99_latency_us > 0);
        assert!(result.p99_latency_us >= result.p95_latency_us);
    }

    #[tokio::test]
    async fn run_result_has_errors() {
        let config = LoadTestConfig::new("http://test").with_total(100);
        let result = LoadTest::run(config).await;
        // 100 طلب، 1 من كل 20 يفشل → ~5 أخطاء.
        assert!(result.failed > 0);
        assert!(result.errors.contains_key("timeout"));
    }

    #[tokio::test]
    async fn run_summary_contains_stats() {
        let config = LoadTestConfig::new("http://test").with_total(50);
        let result = LoadTest::run(config).await;
        let summary = result.summary();
        assert!(summary.contains("RPS"));
        assert!(summary.contains("avg"));
    }

    #[tokio::test]
    async fn run_ramp_up() {
        let results = LoadTest::run_ramp_up("http://test", 5, 20, 3).await;
        assert_eq!(results.len(), 3);
        // كل خطوة لها نتائج.
        for r in &results {
            assert!(r.total_sent > 0);
        }
    }
}
