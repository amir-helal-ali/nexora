//! أنواع التهديدات والمؤشرات.

use serde::{Deserialize, Serialize};

/// نوع التهديد الأمني.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreatType {
    /// محاولات تسجيل دخول فاشلة متكررة (brute force).
    BruteForce,
    /// دخول من IP مشبوه.
    SuspiciousIp,
    /// نشاط غير معتاد.
    Anomaly,
    /// تصعيد صلاحيات مريب.
    PrivilegeEscalation,
    /// وصول في وقت غير معتاد.
    OffHoursAccess,
    /// استخدام متعدد لنفس الحساب من أماكن مختلفة.
    ImpossibleTravel,
    /// تسريب بيانات محتمل.
    DataExfiltration,
    /// استخدام API بمعدل غير معتاد.
    RateLimitAbuse,
    /// محاولة تخطي MFA.
    MfaBypass,
    /// أخرى.
    Other,
}

impl ThreatType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BruteForce => "brute_force",
            Self::SuspiciousIp => "suspicious_ip",
            Self::Anomaly => "anomaly",
            Self::PrivilegeEscalation => "privilege_escalation",
            Self::OffHoursAccess => "off_hours_access",
            Self::ImpossibleTravel => "impossible_travel",
            Self::DataExfiltration => "data_exfiltration",
            Self::RateLimitAbuse => "rate_limit_abuse",
            Self::MfaBypass => "mfa_bypass",
            Self::Other => "other",
        }
    }
}

impl std::fmt::Display for ThreatType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// مؤشر على تهديد (IOC — Indicator of Compromise).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatIndicator {
    /// نوع التهديد.
    pub threat_type: ThreatType,
    /// الوصف.
    pub description: String,
    /// القيمة المشبوهة (IP، user agent، إلخ).
    pub indicator: String,
    /// مستوى الثقة (0.0 - 1.0).
    pub confidence: f64,
}

impl ThreatIndicator {
    pub fn new(threat_type: ThreatType, description: impl Into<String>, indicator: impl Into<String>, confidence: f64) -> Self {
        Self {
            threat_type,
            description: description.into(),
            indicator: indicator.into(),
            confidence: confidence.clamp(0.0, 1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threat_type_as_str() {
        assert_eq!(ThreatType::BruteForce.as_str(), "brute_force");
        assert_eq!(ThreatType::SuspiciousIp.as_str(), "suspicious_ip");
        assert_eq!(ThreatType::MfaBypass.as_str(), "mfa_bypass");
    }

    #[test]
    fn threat_type_display() {
        assert_eq!(ThreatType::Anomaly.to_string(), "anomaly");
    }

    #[test]
    fn threat_indicator_new() {
        let ti = ThreatIndicator::new(
            ThreatType::BruteForce,
            "5 محاولات فاشلة",
            "192.168.1.1",
            0.95,
        );
        assert_eq!(ti.threat_type, ThreatType::BruteForce);
        assert_eq!(ti.indicator, "192.168.1.1");
        assert!((ti.confidence - 0.95).abs() < 0.001);
    }

    #[test]
    fn confidence_clamped() {
        let ti = ThreatIndicator::new(ThreatType::Other, "x", "y", 1.5);
        assert_eq!(ti.confidence, 1.0);
        let ti2 = ThreatIndicator::new(ThreatType::Other, "x", "y", -0.5);
        assert_eq!(ti2.confidence, 0.0);
    }

    #[test]
    fn serde_roundtrip() {
        let ti = ThreatIndicator::new(ThreatType::SuspiciousIp, "test", "1.2.3.4", 0.8);
        let json = serde_json::to_string(&ti).unwrap();
        let back: ThreatIndicator = serde_json::from_str(&json).unwrap();
        assert_eq!(ti.threat_type, back.threat_type);
        assert_eq!(ti.indicator, back.indicator);
    }
}
