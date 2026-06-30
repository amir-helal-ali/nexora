//! Health Monitor — liveness, readiness, and stats for the Core.
//!
//! See Nexora Engineering Specification, Part 4 (SELF HEALING) and Part 13
//! (SYSTEM HEALTH MODEL). The Core constantly verifies health and triggers
//! self-healing when problems are detected.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use time::OffsetDateTime;

/// Overall health status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// All subsystems healthy.
    Healthy,
    /// Some subsystem degraded but Core is operational.
    Degraded,
    /// Critical subsystem failing.
    Unhealthy,
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Healthy => f.write_str("healthy"),
            Self::Degraded => f.write_str("degraded"),
            Self::Unhealthy => f.write_str("unhealthy"),
        }
    }
}

/// Health of a single subsystem.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubsystemHealth {
    /// Subsystem name (e.g. `module_manager`, `event_bus`).
    pub name: String,
    /// Current status.
    pub status: HealthStatus,
    /// Last check timestamp (unix nanos).
    pub last_check: i64,
    /// Optional message (e.g. error description).
    pub message: Option<String>,
}

/// The Health Monitor. Thread-safe.
pub struct HealthMonitor {
    subsystems: RwLock<HashMap<String, SubsystemHealth>>,
}

impl fmt::Debug for HealthMonitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.subsystems.read().len();
        f.debug_struct("HealthMonitor")
            .field("subsystems", &count)
            .field("status", &self.status())
            .finish()
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthMonitor {
    /// Construct an empty health monitor.
    pub fn new() -> Self {
        Self {
            subsystems: RwLock::new(HashMap::new()),
        }
    }

    /// Register or update a subsystem's health.
    pub fn report(&self, name: impl Into<String>, status: HealthStatus, message: Option<String>) {
        let name = name.into();
        let mut subs = self.subsystems.write();
        let entry = SubsystemHealth {
            name: name.clone(),
            status,
            last_check: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            message,
        };
        subs.insert(name, entry);
    }

    /// Get the overall status: worst of all subsystems.
    pub fn status(&self) -> HealthStatus {
        let subs = self.subsystems.read();
        if subs.is_empty() {
            return HealthStatus::Healthy;
        }
        let mut worst = HealthStatus::Healthy;
        for s in subs.values() {
            match s.status {
                HealthStatus::Unhealthy => return HealthStatus::Unhealthy,
                HealthStatus::Degraded => worst = HealthStatus::Degraded,
                HealthStatus::Healthy => {}
            }
        }
        worst
    }

    /// Snapshot all subsystem health.
    pub fn snapshot(&self) -> Vec<SubsystemHealth> {
        self.subsystems.read().values().cloned().collect()
    }

    /// Returns `true` if all subsystems are healthy.
    pub fn is_healthy(&self) -> bool {
        self.status() == HealthStatus::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_monitor_is_healthy() {
        let m = HealthMonitor::new();
        assert_eq!(m.status(), HealthStatus::Healthy);
        assert!(m.is_healthy());
    }

    #[test]
    fn degraded_propagates() {
        let m = HealthMonitor::new();
        m.report("a", HealthStatus::Healthy, None);
        m.report("b", HealthStatus::Degraded, Some("slow".into()));
        assert_eq!(m.status(), HealthStatus::Degraded);
    }

    #[test]
    fn unhealthy_dominates() {
        let m = HealthMonitor::new();
        m.report("a", HealthStatus::Healthy, None);
        m.report("b", HealthStatus::Degraded, Some("slow".into()));
        m.report("c", HealthStatus::Unhealthy, Some("down".into()));
        assert_eq!(m.status(), HealthStatus::Unhealthy);
        assert!(!m.is_healthy());
    }

    #[test]
    fn snapshot_works() {
        let m = HealthMonitor::new();
        m.report("a", HealthStatus::Healthy, None);
        m.report("b", HealthStatus::Degraded, None);
        let snap = m.snapshot();
        assert_eq!(snap.len(), 2);
    }
}
