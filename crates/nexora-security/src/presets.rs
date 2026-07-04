//! # السياسات الافتراضية الجاهزة
//!
//! مجموعة من السياسات الأمنية الجاهزة التي يمكن تطبيقها بنقرة واحدة.
//! توفر حماية أساسية للمنصة بدون الحاجة لتكوين يدوي.

use crate::policies::{PolicyAction, PolicyType, SecurityPolicy};
use crate::alert::Severity;

/// مجموعة سياسات جاهزة.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetBundle {
    /// الحماية الأساسية (مناسب لمعظم المنصات).
    Basic,
    /// حماية المؤسسات (صارمة، للمؤسسات الكبيرة).
    Enterprise,
    /// حماية عالية (للأنظمة الحساسة).
    HighSecurity,
}

impl PresetBundle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Enterprise => "enterprise",
            Self::HighSecurity => "high_security",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Basic => "حماية أساسية: قفل الحساب + حد المعدل + سياسة كلمات مرور",
            Self::Enterprise => "حماية مؤسسية: MFA إجباري + قيود وقت + قفل حساب صارم",
            Self::HighSecurity => "حماية عالية: MFA + WebAuthn + انتهاء كلمة مرور + جلسات قصيرة",
        }
    }
}

/// إنشاء سياسات حزمة جاهزة.
pub fn create_preset(bundle: PresetBundle) -> Vec<SecurityPolicy> {
    match bundle {
        PresetBundle::Basic => vec![
            SecurityPolicy::new("قفل الحساب", PolicyType::AccountLockout, PolicyAction::Deny)
                .with_description("قفل الحساب بعد 5 محاولات فاشلة في 5 دقائق")
                .with_severity(Severity::High)
                .with_parameter("max_attempts", "5")
                .with_parameter("window_seconds", "300")
                .with_resource("api/auth/*"),
            SecurityPolicy::new("حد المعدل", PolicyType::RateLimit, PolicyAction::Deny)
                .with_description("100 طلب/دقيقة لكل IP")
                .with_severity(Severity::Medium)
                .with_parameter("max_requests", "100")
                .with_parameter("window_seconds", "60"),
            SecurityPolicy::new("سياسة كلمات المرور", PolicyType::PasswordPolicy, PolicyAction::Warn)
                .with_description("8 أحرف على الأقل، أحرف كبيرة وصغيرة وأرقام")
                .with_severity(Severity::Low)
                .with_parameter("min_length", "8")
                .with_parameter("require_uppercase", "true")
                .with_parameter("require_lowercase", "true")
                .with_parameter("require_digits", "true"),
        ],
        PresetBundle::Enterprise => vec![
            SecurityPolicy::new("MFA إجباري للإدارة", PolicyType::RequireMfa, PolicyAction::Deny)
                .with_description("مطلوب MFA لكل المسارات الإدارية")
                .with_severity(Severity::Critical)
                .with_resource("api/admin/*")
                .with_resource("api/security/*")
                .with_resource("api/auth/mfa/*"),
            SecurityPolicy::new("قفل الحساب الصارم", PolicyType::AccountLockout, PolicyAction::Deny)
                .with_description("قفل بعد 3 محاولات فاشلة في 10 دقائق")
                .with_severity(Severity::Critical)
                .with_parameter("max_attempts", "3")
                .with_parameter("window_seconds", "600")
                .with_resource("api/auth/*"),
            SecurityPolicy::new("قيود وقت الوصول", PolicyType::TimeRestriction, PolicyAction::Warn)
                .with_description("تحذير للوصول خارج ساعات العمل (8-18)")
                .with_severity(Severity::Medium)
                .with_parameter("work_start", "8")
                .with_parameter("work_end", "18")
                .with_resource("api/admin/*"),
            SecurityPolicy::new("حد الجلسات", PolicyType::MaxSessions, PolicyAction::Deny)
                .with_description("حد أقصى 3 جلسات نشطة لكل مستخدم")
                .with_severity(Severity::High)
                .with_parameter("max_sessions", "3"),
            SecurityPolicy::new("حد المعدل الصارم", PolicyType::RateLimit, PolicyAction::Deny)
                .with_description("50 طلب/دقيقة لكل IP")
                .with_severity(Severity::Medium)
                .with_parameter("max_requests", "50")
                .with_parameter("window_seconds", "60"),
        ],
        PresetBundle::HighSecurity => vec![
            SecurityPolicy::new("MFA إجباري شامل", PolicyType::RequireMfa, PolicyAction::Deny)
                .with_description("MFA مطلوب لكل المسارات المحمية")
                .with_severity(Severity::Critical)
                .with_resource("api/*"),
            SecurityPolicy::new("انتهاء كلمة المرور", PolicyType::PasswordExpiry, PolicyAction::Deny)
                .with_description("كلمات المرور تنتهي بعد 90 يوماً")
                .with_severity(Severity::High)
                .with_parameter("max_age_days", "90"),
            SecurityPolicy::new("جلسات قصيرة", PolicyType::SessionPolicy, PolicyAction::Deny)
                .with_description("انتهاء الجلسة بعد 30 دقيقة من عدم النشاط")
                .with_severity(Severity::High)
                .with_parameter("ttl_minutes", "30"),
            SecurityPolicy::new("قفل الحساب الفائق", PolicyType::AccountLockout, PolicyAction::Deny)
                .with_description("قفل بعد 2 محاولات فاشلة في 15 دقيقة")
                .with_severity(Severity::Critical)
                .with_parameter("max_attempts", "2")
                .with_parameter("window_seconds", "900")
                .with_resource("api/auth/*"),
            SecurityPolicy::new("قيود IP للإدارة", PolicyType::IpRestriction, PolicyAction::Deny)
                .with_description("الإدارة فقط من شبكات موثوقة")
                .with_severity(Severity::Critical)
                .with_resource("api/admin/*")
                .with_parameter("allowed_cidrs", "10.0.0.0/8,172.16.0.0/12,192.168.0.0/16"),
            SecurityPolicy::new("كلمات مرور قوية جداً", PolicyType::PasswordPolicy, PolicyAction::Deny)
                .with_description("16 حرف، أحرف كبيرة وصغيرة وأرقام ورموز")
                .with_severity(Severity::High)
                .with_parameter("min_length", "16")
                .with_parameter("require_uppercase", "true")
                .with_parameter("require_lowercase", "true")
                .with_parameter("require_digits", "true")
                .with_parameter("require_symbols", "true"),
        ],
    }
}

/// قائمة كل الحزم الجاهزة.
pub fn all_presets() -> Vec<(PresetBundle, &'static str, &'static str)> {
    vec![
        (PresetBundle::Basic, PresetBundle::Basic.as_str(), PresetBundle::Basic.description()),
        (PresetBundle::Enterprise, PresetBundle::Enterprise.as_str(), PresetBundle::Enterprise.description()),
        (PresetBundle::HighSecurity, PresetBundle::HighSecurity.as_str(), PresetBundle::HighSecurity.description()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_preset_has_3_policies() {
        let policies = create_preset(PresetBundle::Basic);
        assert_eq!(policies.len(), 3);
    }

    #[test]
    fn enterprise_preset_has_5_policies() {
        let policies = create_preset(PresetBundle::Enterprise);
        assert_eq!(policies.len(), 5);
    }

    #[test]
    fn high_security_preset_has_6_policies() {
        let policies = create_preset(PresetBundle::HighSecurity);
        assert_eq!(policies.len(), 6);
    }

    #[test]
    fn basic_has_account_lockout() {
        let policies = create_preset(PresetBundle::Basic);
        assert!(policies.iter().any(|p| p.policy_type == PolicyType::AccountLockout));
    }

    #[test]
    fn enterprise_has_mfa() {
        let policies = create_preset(PresetBundle::Enterprise);
        assert!(policies.iter().any(|p| p.policy_type == PolicyType::RequireMfa));
    }

    #[test]
    fn high_security_has_password_expiry() {
        let policies = create_preset(PresetBundle::HighSecurity);
        assert!(policies.iter().any(|p| p.policy_type == PolicyType::PasswordExpiry));
    }

    #[test]
    fn high_security_has_ip_restriction() {
        let policies = create_preset(PresetBundle::HighSecurity);
        assert!(policies.iter().any(|p| p.policy_type == PolicyType::IpRestriction));
    }

    #[test]
    fn all_policies_enabled_by_default() {
        for bundle in [PresetBundle::Basic, PresetBundle::Enterprise, PresetBundle::HighSecurity] {
            let policies = create_preset(bundle);
            for p in &policies {
                assert!(p.enabled, "السياسة {} غير مفعّلة", p.name);
            }
        }
    }

    #[test]
    fn all_policies_have_names() {
        for bundle in [PresetBundle::Basic, PresetBundle::Enterprise, PresetBundle::HighSecurity] {
            let policies = create_preset(bundle);
            for p in &policies {
                assert!(!p.name.is_empty());
            }
        }
    }

    #[test]
    fn preset_as_str() {
        assert_eq!(PresetBundle::Basic.as_str(), "basic");
        assert_eq!(PresetBundle::Enterprise.as_str(), "enterprise");
        assert_eq!(PresetBundle::HighSecurity.as_str(), "high_security");
    }

    #[test]
    fn preset_descriptions() {
        assert!(PresetBundle::Basic.description().contains("أساسي"));
        assert!(PresetBundle::Enterprise.description().contains("مؤسس"));
        assert!(PresetBundle::HighSecurity.description().contains("عالي"));
    }

    #[test]
    fn all_presets_list() {
        let presets = all_presets();
        assert_eq!(presets.len(), 3);
    }

    #[test]
    fn basic_preset_severities() {
        let policies = create_preset(PresetBundle::Basic);
        let high_count = policies.iter().filter(|p| p.severity == Severity::High).count();
        assert!(high_count >= 1);
    }

    #[test]
    fn high_security_all_critical_or_high() {
        let policies = create_preset(PresetBundle::HighSecurity);
        for p in &policies {
            assert!(
                p.severity == Severity::Critical || p.severity == Severity::High,
                "السياسة {} ليست حرجة أو عالية",
                p.name
            );
        }
    }

    #[test]
    fn enterprise_has_time_restriction() {
        let policies = create_preset(PresetBundle::Enterprise);
        assert!(policies.iter().any(|p| p.policy_type == PolicyType::TimeRestriction));
    }

    #[test]
    fn policies_have_resources() {
        let enterprise = create_preset(PresetBundle::Enterprise);
        let mfa_policy = enterprise.iter().find(|p| p.policy_type == PolicyType::RequireMfa).unwrap();
        assert!(!mfa_policy.resources.is_empty());
    }

    #[test]
    fn policies_have_parameters() {
        let basic = create_preset(PresetBundle::Basic);
        let lockout = basic.iter().find(|p| p.policy_type == PolicyType::AccountLockout).unwrap();
        assert!(!lockout.parameters.is_empty());
        assert_eq!(lockout.parameters.get("max_attempts").unwrap(), "5");
    }
}
