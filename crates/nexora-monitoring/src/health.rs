//! فحوصات الصحة.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use time::OffsetDateTime;

/// حالة الصحة.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// صحية.
    Healthy,
    /// متدهورة (تحذير).
    Degraded,
    /// غير صحية.
    Unhealthy,
}

impl HealthStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Unhealthy => "unhealthy",
        }
    }
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// نتيجة فحص صحة.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    /// اسم الفحص.
    pub name: String,
    /// الحالة.
    pub status: HealthStatus,
    /// رسالة.
    pub message: String,
    /// وقت الفحص (unix nanos).
    pub checked_at: i64,
    /// مدة الفحص (ميكروثانية).
    pub duration_us: u64,
}

impl ProbeResult {
    pub fn healthy(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Healthy,
            message: message.into(),
            checked_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            duration_us: 0,
        }
    }

    pub fn degraded(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Degraded,
            message: message.into(),
            checked_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            duration_us: 0,
        }
    }

    pub fn unhealthy(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Unhealthy,
            message: message.into(),
            checked_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            duration_us: 0,
        }
    }
}

/// فحص صحة (trait قابل للتطبيق).
pub trait HealthProbe: Send + Sync {
    /// اسم الفحص.
    fn name(&self) -> &str;

    /// تنفيذ الفحص.
    fn check(&self) -> ProbeResult;
}

/// مدير فحوصات الصحة.
pub struct HealthProbeManager {
    probes: RwLock<Vec<Arc<dyn HealthProbe>>>,
    last_results: RwLock<HashMap<String, ProbeResult>>,
}

impl Default for HealthProbeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthProbeManager {
    pub fn new() -> Self {
        Self {
            probes: RwLock::new(Vec::new()),
            last_results: RwLock::new(HashMap::new()),
        }
    }

    /// تسجيل فحص.
    pub fn register(&self, probe: Arc<dyn HealthProbe>) {
        self.probes.write().push(probe);
    }

    /// تنفيذ كل الفحوصات.
    pub fn run_all(&self) -> Vec<ProbeResult> {
        let probes = self.probes.read().clone();
        let mut results = Vec::new();
        let mut last = self.last_results.write();
        for probe in &probes {
            let result = probe.check();
            last.insert(probe.name().to_string(), result.clone());
            results.push(result);
        }
        results
    }

    /// الحالة الإجمالية (أسوأ حالة).
    pub fn overall_status(&self) -> HealthStatus {
        let results = self.last_results.read();
        if results.is_empty() {
            return HealthStatus::Healthy;
        }
        let mut worst = HealthStatus::Healthy;
        for r in results.values() {
            match r.status {
                HealthStatus::Unhealthy => return HealthStatus::Unhealthy,
                HealthStatus::Degraded => worst = HealthStatus::Degraded,
                _ => {}
            }
        }
        worst
    }

    /// آخر نتائج.
    pub fn last_results(&self) -> Vec<ProbeResult> {
        self.last_results.read().values().cloned().collect()
    }

    /// عدد الفحوصات.
    pub fn probe_count(&self) -> usize {
        self.probes.read().len()
    }
}

/// فحص بسيط (closure-based).
pub struct SimpleProbe {
    name: String,
    check_fn: Box<dyn Fn() -> ProbeResult + Send + Sync>,
}

impl SimpleProbe {
    pub fn new<F>(name: impl Into<String>, f: F) -> Self
    where
        F: Fn() -> ProbeResult + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            check_fn: Box::new(f),
        }
    }
}

impl HealthProbe for SimpleProbe {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(&self) -> ProbeResult {
        (self.check_fn)()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_status_as_str() {
        assert_eq!(HealthStatus::Healthy.as_str(), "healthy");
        assert_eq!(HealthStatus::Degraded.as_str(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.as_str(), "unhealthy");
    }

    #[test]
    fn probe_result_healthy() {
        let r = ProbeResult::healthy("test", "ok");
        assert_eq!(r.status, HealthStatus::Healthy);
        assert_eq!(r.message, "ok");
    }

    #[test]
    fn probe_result_unhealthy() {
        let r = ProbeResult::unhealthy("test", "fail");
        assert_eq!(r.status, HealthStatus::Unhealthy);
    }

    #[test]
    fn manager_empty_healthy() {
        let mgr = HealthProbeManager::new();
        assert_eq!(mgr.overall_status(), HealthStatus::Healthy);
    }

    #[test]
    fn manager_register_and_run() {
        let mgr = HealthProbeManager::new();
        mgr.register(Arc::new(SimpleProbe::new("db", || {
            ProbeResult::healthy("db", "connected")
        })));
        let results = mgr.run_all();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, HealthStatus::Healthy);
    }

    #[test]
    fn manager_overall_degraded() {
        let mgr = HealthProbeManager::new();
        mgr.register(Arc::new(SimpleProbe::new("ok", || {
            ProbeResult::healthy("ok", "fine")
        })));
        mgr.register(Arc::new(SimpleProbe::new("warn", || {
            ProbeResult::degraded("warn", "slow")
        })));
        mgr.run_all();
        assert_eq!(mgr.overall_status(), HealthStatus::Degraded);
    }

    #[test]
    fn manager_overall_unhealthy() {
        let mgr = HealthProbeManager::new();
        mgr.register(Arc::new(SimpleProbe::new("ok", || {
            ProbeResult::healthy("ok", "fine")
        })));
        mgr.register(Arc::new(SimpleProbe::new("down", || {
            ProbeResult::unhealthy("down", "offline")
        })));
        mgr.run_all();
        assert_eq!(mgr.overall_status(), HealthStatus::Unhealthy);
    }

    #[test]
    fn manager_last_results() {
        let mgr = HealthProbeManager::new();
        mgr.register(Arc::new(SimpleProbe::new("a", || {
            ProbeResult::healthy("a", "ok")
        })));
        mgr.run_all();
        let results = mgr.last_results();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn manager_probe_count() {
        let mgr = HealthProbeManager::new();
        mgr.register(Arc::new(SimpleProbe::new("a", || ProbeResult::healthy("a", ""))));
        mgr.register(Arc::new(SimpleProbe::new("b", || ProbeResult::healthy("b", ""))));
        assert_eq!(mgr.probe_count(), 2);
    }

    #[test]
    fn simple_probe_executes() {
        let probe = SimpleProbe::new("test", || ProbeResult::healthy("test", "works"));
        assert_eq!(probe.name(), "test");
        let r = probe.check();
        assert_eq!(r.status, HealthStatus::Healthy);
    }
}
