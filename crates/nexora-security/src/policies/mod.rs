//! # محرك السياسات الأمنية
//!
//! سياسات قابلة للتكوين تتحكم في سلوك النظام الأمني. يمكن للمشرفين
//! تعريف سياسات مثل:
//! - "مطلوب MFA للوصول للإعدادات الحساسة"
//! - "منع الدخول من خارج ساعات العمل (مع استثناءات)"
//! - "حد أقصى 5 جلسات نشطة لكل مستخدم"
//! - "قفل الحساب بعد 10 محاولات فاشلة"

use crate::alert::Severity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use time::OffsetDateTime;

/// نوع السياسة.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyType {
    /// مطلوب MFA لإجراء معين.
    RequireMfa,
    /// قفل الحساب بعد عدد محاولات فاشلة.
    AccountLockout,
    /// حد الجلسات النشطة.
    MaxSessions,
    /// قيود وقت الوصول.
    TimeRestriction,
    /// قيود IP.
    IpRestriction,
    /// حد معدل الطلبات.
    RateLimit,
    /// منع كلمات المرور الضعيفة.
    PasswordPolicy,
    /// انتهاء صلاحية كلمة المرور.
    PasswordExpiry,
    /// سياسة الجلسات (TTL).
    SessionPolicy,
    /// أخرى.
    Custom,
}

impl PolicyType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RequireMfa => "require_mfa",
            Self::AccountLockout => "account_lockout",
            Self::MaxSessions => "max_sessions",
            Self::TimeRestriction => "time_restriction",
            Self::IpRestriction => "ip_restriction",
            Self::RateLimit => "rate_limit",
            Self::PasswordPolicy => "password_policy",
            Self::PasswordExpiry => "password_expiry",
            Self::SessionPolicy => "session_policy",
            Self::Custom => "custom",
        }
    }
}

/// إجراء سياسة.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    /// السماح (تمرير).
    Allow,
    /// المنع.
    Deny,
    /// تحذير فقط (تسجيل + متابعة).
    Warn,
    /// طلب مصادقة إضافية (step-up).
    RequireStepUp,
}

impl Default for PolicyAction {
    fn default() -> Self {
        Self::Allow
    }
}

/// سياسة أمنية.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// معرّف فريد.
    pub id: String,
    /// اسم العرض.
    pub name: String,
    /// الوصف.
    pub description: String,
    /// نوع السياسة.
    pub policy_type: PolicyType,
    /// الإجراء عند المطابقة.
    pub action: PolicyAction,
    /// مستوى الخطورة (للتنبيه).
    pub severity: Severity,
    /// هل السياسة مفعّلة؟
    pub enabled: bool,
    /// المعاملات (key-value).
    pub parameters: HashMap<String, String>,
    /// الموارد المطبّقة عليها (مثلاً "api/billing/*").
    pub resources: Vec<String>,
    /// وقت الإنشاء.
    pub created_at: i64,
    /// وقت آخر تعديل.
    pub updated_at: i64,
}

impl SecurityPolicy {
    pub fn new(
        name: impl Into<String>,
        policy_type: PolicyType,
        action: PolicyAction,
    ) -> Self {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            policy_type,
            action,
            severity: Severity::Medium,
            enabled: true,
            parameters: HashMap::new(),
            resources: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_parameter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }

    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resources.push(resource.into());
        self
    }

    /// هل السياسة تنطبق على المورد؟
    pub fn applies_to(&self, resource: &str) -> bool {
        if self.resources.is_empty() {
            return true; // لا موارد = تطبّق على الكل.
        }
        for r in &self.resources {
            if resource.starts_with(r.trim_end_matches('*')) {
                return true;
            }
            if r == resource {
                return true;
            }
        }
        false
    }
}

/// نتيجة تقييم السياسة.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEvaluation {
    /// الإجراء المتخذ.
    pub action: PolicyAction,
    /// السياسة المطابقة (إن وُجدت).
    pub policy_id: Option<String>,
    /// سبب القرار.
    pub reason: String,
    /// مستوى الخطورة.
    pub severity: Severity,
}

impl PolicyEvaluation {
    pub fn allow() -> Self {
        Self {
            action: PolicyAction::Allow,
            policy_id: None,
            reason: "لا توجد سياسة مانعة".into(),
            severity: Severity::Info,
        }
    }

    pub fn deny(policy: &SecurityPolicy, reason: impl Into<String>) -> Self {
        Self {
            action: PolicyAction::Deny,
            policy_id: Some(policy.id.clone()),
            reason: reason.into(),
            severity: policy.severity,
        }
    }

    pub fn warn(policy: &SecurityPolicy, reason: impl Into<String>) -> Self {
        Self {
            action: PolicyAction::Warn,
            policy_id: Some(policy.id.clone()),
            reason: reason.into(),
            severity: policy.severity,
        }
    }

    pub fn is_allowed(&self) -> bool {
        matches!(self.action, PolicyAction::Allow | PolicyAction::Warn)
    }
}

/// محرك السياسات.
pub struct PolicyEngine {
    /// السياسات المسجّلة.
    policies: parking_lot::RwLock<Vec<SecurityPolicy>>,
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyEngine {
    pub fn new() -> Self {
        Self {
            policies: parking_lot::RwLock::new(Vec::new()),
        }
    }

    /// تسجيل سياسة.
    pub fn register(&self, policy: SecurityPolicy) -> String {
        let id = policy.id.clone();
        self.policies.write().push(policy);
        id
    }

    /// حذف سياسة.
    pub fn remove(&self, id: &str) -> bool {
        let mut policies = self.policies.write();
        let before = policies.len();
        policies.retain(|p| p.id != id);
        policies.len() != before
    }

    /// تفعيل/تعطيل سياسة.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> bool {
        let mut policies = self.policies.write();
        for p in policies.iter_mut() {
            if p.id == id {
                p.enabled = enabled;
                p.updated_at = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
                return true;
            }
        }
        false
    }

    /// قائمة كل السياسات.
    pub fn list(&self) -> Vec<SecurityPolicy> {
        self.policies.read().clone()
    }

    /// قائمة السياسات المفعّلة فقط.
    pub fn list_enabled(&self) -> Vec<SecurityPolicy> {
        self.policies.read().iter().filter(|p| p.enabled).cloned().collect()
    }

    /// تقييم السياسات لمورد معين.
    pub fn evaluate(&self, resource: &str, policy_type: Option<PolicyType>) -> PolicyEvaluation {
        let policies = self.policies.read();
        let mut warn_result: Option<PolicyEvaluation> = None;
        for p in policies.iter() {
            if !p.enabled {
                continue;
            }
            if let Some(pt) = policy_type {
                if p.policy_type != pt {
                    continue;
                }
            }
            if !p.applies_to(resource) {
                continue;
            }
            // سياسة مطابقة.
            match p.action {
                PolicyAction::Allow => continue,
                PolicyAction::Deny => {
                    return PolicyEvaluation::deny(p, format!("ممنوع بواسطة: {}", p.name));
                }
                PolicyAction::Warn => {
                    // Warn لا يمنع، لكن نسجّله. نتابع البحث عن سياسات Deny.
                    if warn_result.is_none() {
                        warn_result = Some(PolicyEvaluation::warn(p, format!("تحذير من: {}", p.name)));
                    }
                }
                PolicyAction::RequireStepUp => {
                    return PolicyEvaluation {
                        action: PolicyAction::RequireStepUp,
                        policy_id: Some(p.id.clone()),
                        reason: format!("مطلوب مصادقة إضافية: {}", p.name),
                        severity: p.severity,
                    };
                }
            }
        }
        // إن لم نجد Deny ولكن وجدنا Warn، أرجع Warn.
        // وإلا، أرجع Allow.
        warn_result.unwrap_or_else(PolicyEvaluation::allow)
    }

    /// عدد السياسات.
    pub fn count(&self) -> usize {
        self.policies.read().len()
    }

    /// عدد السياسات المفعّلة.
    pub fn enabled_count(&self) -> usize {
        self.policies.read().iter().filter(|p| p.enabled).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_type_as_str() {
        assert_eq!(PolicyType::RequireMfa.as_str(), "require_mfa");
        assert_eq!(PolicyType::AccountLockout.as_str(), "account_lockout");
    }

    #[test]
    fn policy_new_defaults() {
        let p = SecurityPolicy::new("test", PolicyType::RateLimit, PolicyAction::Deny);
        assert!(p.enabled);
        assert_eq!(p.severity, Severity::Medium);
        assert!(p.resources.is_empty());
        assert!(!p.id.is_empty());
    }

    #[test]
    fn policy_builder() {
        let p = SecurityPolicy::new("test", PolicyType::RequireMfa, PolicyAction::Deny)
            .with_description("وصف")
            .with_severity(Severity::High)
            .with_parameter("min_length", "8")
            .with_resource("api/billing/*");
        assert_eq!(p.description, "وصف");
        assert_eq!(p.severity, Severity::High);
        assert_eq!(p.parameters.get("min_length").unwrap(), "8");
        assert_eq!(p.resources.len(), 1);
    }

    #[test]
    fn applies_to_no_resources_matches_all() {
        let p = SecurityPolicy::new("test", PolicyType::RateLimit, PolicyAction::Deny);
        assert!(p.applies_to("anything"));
        assert!(p.applies_to("api/billing"));
    }

    #[test]
    fn applies_to_specific_resource() {
        let p = SecurityPolicy::new("test", PolicyType::RateLimit, PolicyAction::Deny)
            .with_resource("api/billing/*");
        assert!(p.applies_to("api/billing/invoices"));
        assert!(p.applies_to("api/billing/"));
        assert!(!p.applies_to("api/auth/login"));
    }

    #[test]
    fn policy_evaluation_allow() {
        let e = PolicyEvaluation::allow();
        assert!(e.is_allowed());
        assert_eq!(e.action, PolicyAction::Allow);
    }

    #[test]
    fn policy_evaluation_deny() {
        let p = SecurityPolicy::new("test", PolicyType::RateLimit, PolicyAction::Deny);
        let e = PolicyEvaluation::deny(&p, "تجاوز الحد");
        assert!(!e.is_allowed());
        assert_eq!(e.action, PolicyAction::Deny);
        assert!(e.reason.contains("تجاوز"));
    }

    #[test]
    fn engine_register_and_list() {
        let engine = PolicyEngine::new();
        let p = SecurityPolicy::new("test", PolicyType::RateLimit, PolicyAction::Deny);
        engine.register(p);
        assert_eq!(engine.count(), 1);
        assert_eq!(engine.list().len(), 1);
    }

    #[test]
    fn engine_remove() {
        let engine = PolicyEngine::new();
        let p = SecurityPolicy::new("test", PolicyType::RateLimit, PolicyAction::Deny);
        let id = engine.register(p);
        assert!(engine.remove(&id));
        assert_eq!(engine.count(), 0);
    }

    #[test]
    fn engine_set_enabled() {
        let engine = PolicyEngine::new();
        let p = SecurityPolicy::new("test", PolicyType::RateLimit, PolicyAction::Deny);
        let id = engine.register(p);
        assert!(engine.set_enabled(&id, false));
        assert_eq!(engine.enabled_count(), 0);
        assert!(engine.set_enabled(&id, true));
        assert_eq!(engine.enabled_count(), 1);
    }

    #[test]
    fn evaluate_no_policies_allows() {
        let engine = PolicyEngine::new();
        let e = engine.evaluate("api/anything", None);
        assert!(e.is_allowed());
    }

    #[test]
    fn evaluate_denying_policy() {
        let engine = PolicyEngine::new();
        let p = SecurityPolicy::new("block-billing", PolicyType::Custom, PolicyAction::Deny)
            .with_resource("api/billing/*");
        engine.register(p);

        let e = engine.evaluate("api/billing/invoices", None);
        assert!(!e.is_allowed());
        assert_eq!(e.action, PolicyAction::Deny);
    }

    #[test]
    fn evaluate_warn_does_not_block() {
        let engine = PolicyEngine::new();
        let p = SecurityPolicy::new("warn", PolicyType::Custom, PolicyAction::Warn)
            .with_resource("api/billing/*");
        engine.register(p);

        let e = engine.evaluate("api/billing/invoices", None);
        assert!(e.is_allowed()); // Warn لا يمنع.
        assert_eq!(e.action, PolicyAction::Warn);
    }

    #[test]
    fn evaluate_disabled_policy_ignored() {
        let engine = PolicyEngine::new();
        let p = SecurityPolicy::new("disabled", PolicyType::Custom, PolicyAction::Deny);
        let id = engine.register(p);
        engine.set_enabled(&id, false);

        let e = engine.evaluate("anything", None);
        assert!(e.is_allowed());
    }

    #[test]
    fn evaluate_by_type() {
        let engine = PolicyEngine::new();
        engine.register(
            SecurityPolicy::new("mfa-policy", PolicyType::RequireMfa, PolicyAction::Deny)
                .with_resource("api/billing/*"),
        );
        engine.register(
            SecurityPolicy::new("lockout", PolicyType::AccountLockout, PolicyAction::Deny)
                .with_resource("api/auth/*"),
        );

        // فلترة بالنوع: RequireMfa على billing.
        let e = engine.evaluate("api/billing/invoices", Some(PolicyType::RequireMfa));
        assert!(!e.is_allowed());

        // AccountLockout على billing لا ينطبق (موارد auth فقط).
        let e = engine.evaluate("api/billing/invoices", Some(PolicyType::AccountLockout));
        assert!(e.is_allowed());

        // AccountLockout على auth ينطبق.
        let e = engine.evaluate("api/auth/login", Some(PolicyType::AccountLockout));
        assert!(!e.is_allowed());
    }

    #[test]
    fn evaluate_step_up() {
        let engine = PolicyEngine::new();
        engine.register(
            SecurityPolicy::new("step-up", PolicyType::RequireMfa, PolicyAction::RequireStepUp)
                .with_resource("api/admin/*"),
        );

        let e = engine.evaluate("api/admin/users", None);
        assert_eq!(e.action, PolicyAction::RequireStepUp);
        assert!(e.reason.contains("مصادقة إضافية"));
    }

    #[test]
    fn serde_roundtrip() {
        let p = SecurityPolicy::new("test", PolicyType::RateLimit, PolicyAction::Deny)
            .with_parameter("limit", "100");
        let json = serde_json::to_string(&p).unwrap();
        let back: SecurityPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(p.id, back.id);
        assert_eq!(p.policy_type, back.policy_type);
        assert_eq!(back.parameters.get("limit").unwrap(), "100");
    }

    #[test]
    fn multiple_policies_first_deny_wins() {
        let engine = PolicyEngine::new();
        engine.register(
            SecurityPolicy::new("allow", PolicyType::Custom, PolicyAction::Allow),
        );
        engine.register(
            SecurityPolicy::new("deny", PolicyType::Custom, PolicyAction::Deny),
        );

        let e = engine.evaluate("anything", None);
        assert!(!e.is_allowed());
    }
}
