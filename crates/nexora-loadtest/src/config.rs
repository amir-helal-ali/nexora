//! إعدادات اختبار التحمل.

use serde::{Deserialize, Serialize};

/// إعدادات اختبار التحمل.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadTestConfig {
    /// URL الهدف.
    pub url: String,
    /// الطريقة (GET, POST, إلخ).
    pub method: String,
    /// عدد الطلبات المتزامنة.
    pub concurrent: usize,
    /// إجمالي الطلبات.
    pub total: usize,
    /// مهلة كل طلب (مللي ثانية).
    pub timeout_ms: u64,
    /// ترويسات إضافية.
    pub headers: Vec<(String, String)>,
    /// جسم الطلب (لـ POST).
    pub body: Option<String>,
}

impl Default for LoadTestConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:8080/api/health".into(),
            method: "GET".into(),
            concurrent: 10,
            total: 100,
            timeout_ms: 5000,
            headers: Vec::new(),
            body: None,
        }
    }
}

impl LoadTestConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }

    pub fn with_concurrent(mut self, concurrent: usize) -> Self {
        self.concurrent = concurrent.max(1);
        self
    }

    pub fn with_total(mut self, total: usize) -> Self {
        self.total = total.max(1);
        self
    }

    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = method.into();
        self
    }

    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// عدد الطلبات لكل عامل متزامن.
    pub fn requests_per_worker(&self) -> usize {
        (self.total / self.concurrent).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let c = LoadTestConfig::default();
        assert_eq!(c.concurrent, 10);
        assert_eq!(c.total, 100);
        assert_eq!(c.method, "GET");
    }

    #[test]
    fn builder_methods() {
        let c = LoadTestConfig::new("http://test")
            .with_concurrent(50)
            .with_total(1000)
            .with_method("POST")
            .with_timeout(3000)
            .with_header("Authorization", "Bearer token")
            .with_body(r#"{"key":"value"}"#);
        assert_eq!(c.url, "http://test");
        assert_eq!(c.concurrent, 50);
        assert_eq!(c.total, 1000);
        assert_eq!(c.method, "POST");
        assert_eq!(c.timeout_ms, 3000);
        assert_eq!(c.headers.len(), 1);
        assert!(c.body.is_some());
    }

    #[test]
    fn concurrent_minimum_1() {
        let c = LoadTestConfig::new("test").with_concurrent(0);
        assert_eq!(c.concurrent, 1);
    }

    #[test]
    fn total_minimum_1() {
        let c = LoadTestConfig::new("test").with_total(0);
        assert_eq!(c.total, 1);
    }

    #[test]
    fn requests_per_worker() {
        let c = LoadTestConfig::new("test").with_concurrent(10).with_total(100);
        assert_eq!(c.requests_per_worker(), 10);
    }

    #[test]
    fn requests_per_worker_rounds_down() {
        let c = LoadTestConfig::new("test").with_concurrent(3).with_total(10);
        assert_eq!(c.requests_per_worker(), 3);
    }

    #[test]
    fn serde_roundtrip() {
        let c = LoadTestConfig::new("http://test").with_concurrent(20);
        let json = serde_json::to_string(&c).unwrap();
        let back: LoadTestConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(c.url, back.url);
        assert_eq!(c.concurrent, back.concurrent);
    }
}
