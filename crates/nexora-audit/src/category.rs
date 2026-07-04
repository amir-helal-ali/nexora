//! فئات التدقيق.

use serde::{Deserialize, Serialize};

/// فئة الإجراء المُدقّق.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditCategory {
    /// مصادقة (دخول، خروج، تحديث رمز).
    Auth,
    /// إدارة مستخدمين (إنشاء، حذف، تعديل صلاحيات).
    UserManagement,
    /// وحدات (تثبيت، تفعيل، تعطيل).
    Module,
    /// متجر (نشر، تثبيت حزمة).
    Marketplace,
    /// فوترة (فاتورة، دفعة، اشتراك).
    Billing,
    /// تكوين (تغيير إعدادات).
    Config,
    /// أسرار (إنشاء، تدوير، حذف).
    Secret,
    /// SSO (تفعيل مزود، دخول).
    Sso,
    /// إشعارات (إرسال، تعطيل قناة).
    Notification,
    /// قواعد أتمتة (إنشاء، تعديل، حذف).
    Rule,
    /// ملفات وبيانات (قراءة، كتابة، حذف).
    Data,
    /// نظام (إقلاع، إيقاف، خطأ).
    System,
    /// أخرى.
    Other,
}

impl AuditCategory {
    /// كل الفئات.
    pub const ALL: &'static [AuditCategory] = &[
        AuditCategory::Auth,
        AuditCategory::UserManagement,
        AuditCategory::Module,
        AuditCategory::Marketplace,
        AuditCategory::Billing,
        AuditCategory::Config,
        AuditCategory::Secret,
        AuditCategory::Sso,
        AuditCategory::Notification,
        AuditCategory::Rule,
        AuditCategory::Data,
        AuditCategory::System,
        AuditCategory::Other,
    ];

    /// تمثيل نصي.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::UserManagement => "user_management",
            Self::Module => "module",
            Self::Marketplace => "marketplace",
            Self::Billing => "billing",
            Self::Config => "config",
            Self::Secret => "secret",
            Self::Sso => "sso",
            Self::Notification => "notification",
            Self::Rule => "rule",
            Self::Data => "data",
            Self::System => "system",
            Self::Other => "other",
        }
    }
}

impl std::fmt::Display for AuditCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for AuditCategory {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| format!("فئة تدقيق غير معروفة: {s}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_categories_have_strings() {
        for c in AuditCategory::ALL {
            assert!(!c.as_str().is_empty());
        }
    }

    #[test]
    fn roundtrip_all_categories() {
        for c in AuditCategory::ALL {
            let s = c.as_str();
            let back: AuditCategory = s.parse().unwrap();
            assert_eq!(*c, back);
        }
    }

    #[test]
    fn unknown_category_fails() {
        assert!("nonexistent".parse::<AuditCategory>().is_err());
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(AuditCategory::Auth.to_string(), "auth");
        assert_eq!(AuditCategory::Billing.to_string(), "billing");
    }

    #[test]
    fn serde_roundtrip() {
        let c = AuditCategory::Sso;
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, "\"sso\"");
        let back: AuditCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn all_count() {
        assert_eq!(AuditCategory::ALL.len(), 13);
    }
}
