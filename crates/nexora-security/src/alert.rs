//! التنبيهات الأمنية.

use crate::threat::{ThreatIndicator, ThreatType};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// مستوى خطورة التنبيه.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// معلوماتي.
    Info,
    /// تحذير منخفض.
    Low,
    /// تحذير متوسط.
    Medium,
    /// تحذير عالي.
    High,
    /// حرج.
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    pub fn from_confidence(confidence: f64) -> Self {
        if confidence >= 0.9 {
            Self::Critical
        } else if confidence >= 0.7 {
            Self::High
        } else if confidence >= 0.5 {
            Self::Medium
        } else if confidence >= 0.3 {
            Self::Low
        } else {
            Self::Info
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// حالة التنبيه.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertStatus {
    /// تنبيه نشط (جديد).
    Active,
    /// قيد المراجعة.
    Investigating,
    /// تم حله (إيجابي كاذب).
    Resolved,
    /// تم تجاهله.
    Dismissed,
}

impl Default for AlertStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// تنبيه أمني.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAlert {
    /// معرّف فريد.
    pub id: String,
    /// الفاعل الذي أطلق التنبيه.
    pub actor: String,
    /// مستوى الخطورة.
    pub severity: Severity,
    /// نوع التهديد.
    pub threat_type: ThreatType,
    /// الوصف.
    pub description: String,
    /// المؤشرات المرتبطة.
    pub indicators: Vec<ThreatIndicator>,
    /// الحالة.
    pub status: AlertStatus,
    /// وقت الإنشاء (unix nanos).
    pub created_at: i64,
    /// وقت الحل (إن وُجد).
    pub resolved_at: Option<i64>,
    /// ملاحظات المحلل.
    pub notes: String,
}

impl SecurityAlert {
    pub fn new(
        actor: impl Into<String>,
        severity: Severity,
        threat_type: ThreatType,
        description: impl Into<String>,
        indicators: Vec<ThreatIndicator>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            actor: actor.into(),
            severity,
            threat_type,
            description: description.into(),
            indicators,
            status: AlertStatus::default(),
            created_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            resolved_at: None,
            notes: String::new(),
        }
    }

    pub fn resolve(&mut self) {
        self.status = AlertStatus::Resolved;
        self.resolved_at = Some(OffsetDateTime::now_utc().unix_timestamp_nanos() as i64);
    }

    pub fn dismiss(&mut self) {
        self.status = AlertStatus::Dismissed;
        self.resolved_at = Some(OffsetDateTime::now_utc().unix_timestamp_nanos() as i64);
    }

    pub fn is_active(&self) -> bool {
        self.status == AlertStatus::Active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_ordering() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Medium > Severity::Low);
        assert!(Severity::Low > Severity::Info);
    }

    #[test]
    fn severity_from_confidence() {
        assert_eq!(Severity::from_confidence(0.95), Severity::Critical);
        assert_eq!(Severity::from_confidence(0.75), Severity::High);
        assert_eq!(Severity::from_confidence(0.55), Severity::Medium);
        assert_eq!(Severity::from_confidence(0.35), Severity::Low);
        assert_eq!(Severity::from_confidence(0.15), Severity::Info);
    }

    #[test]
    fn alert_new_defaults() {
        let a = SecurityAlert::new(
            "user-1",
            Severity::High,
            ThreatType::BruteForce,
            "test",
            vec![],
        );
        assert_eq!(a.status, AlertStatus::Active);
        assert!(a.is_active());
        assert!(a.resolved_at.is_none());
        assert!(!a.id.is_empty());
    }

    #[test]
    fn alert_resolve() {
        let mut a = SecurityAlert::new("u", Severity::Low, ThreatType::Other, "x", vec![]);
        a.resolve();
        assert_eq!(a.status, AlertStatus::Resolved);
        assert!(!a.is_active());
        assert!(a.resolved_at.is_some());
    }

    #[test]
    fn alert_dismiss() {
        let mut a = SecurityAlert::new("u", Severity::Low, ThreatType::Other, "x", vec![]);
        a.dismiss();
        assert_eq!(a.status, AlertStatus::Dismissed);
        assert!(!a.is_active());
    }

    #[test]
    fn serde_roundtrip() {
        let a = SecurityAlert::new(
            "alice",
            Severity::Critical,
            ThreatType::SuspiciousIp,
            "IP مشبوه",
            vec![ThreatIndicator::new(ThreatType::SuspiciousIp, "test", "1.2.3.4", 0.9)],
        );
        let json = serde_json::to_string(&a).unwrap();
        let back: SecurityAlert = serde_json::from_str(&json).unwrap();
        assert_eq!(a.id, back.id);
        assert_eq!(a.severity, back.severity);
        assert_eq!(a.threat_type, back.threat_type);
    }

    #[test]
    fn unique_ids() {
        let a1 = SecurityAlert::new("u", Severity::Low, ThreatType::Other, "x", vec![]);
        let a2 = SecurityAlert::new("u", Severity::Low, ThreatType::Other, "x", vec![]);
        assert_ne!(a1.id, a2.id);
    }
}
