//! فلترة سجل التدقيق.

use crate::category::AuditCategory;
use serde::{Deserialize, Serialize};

/// ترتيب النتائج.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditSort {
    /// الأحدث أولاً (تنازلي حسب الطابع الزمني).
    NewestFirst,
    /// الأقدم أولاً (تصاعدي حسب الطابع الزمني).
    OldestFirst,
}

impl Default for AuditSort {
    fn default() -> Self {
        Self::NewestFirst
    }
}

/// فلتر للاستعلام عن مدخلات التدقيق.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditFilter {
    /// فلترة حسب الفاعل (مطابقة تامة).
    pub actor: Option<String>,
    /// فلترة حسب الإجراء (مطابقة تامة).
    pub action: Option<String>,
    /// فلترة حسب الهدف (مطابقة تامة).
    pub target: Option<String>,
    /// فلترة حسب الفئة.
    pub category: Option<AuditCategory>,
    /// فلترة حسب النجاح فقط.
    pub success_only: Option<bool>,
    /// الطابع الزمني الأدنى (unix nanos).
    pub from_timestamp: Option<i64>,
    /// الطابع الزمني الأقصى (unix nanos).
    pub to_timestamp: Option<i64>,
    /// عدد النتائج (افتراضياً 100).
    pub limit: Option<usize>,
    /// إزاحة للترقيم.
    pub offset: Option<usize>,
    /// ترتيب النتائج.
    pub sort: AuditSort,
}

impl AuditFilter {
    /// إنشاء فلتر فارغ (يطابق كل شيء).
    pub fn new() -> Self {
        Self::default()
    }

    /// فلترة حسب الفاعل.
    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    /// فلترة حسب الإجراء.
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    /// فلترة حسب الهدف.
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// فلترة حسب الفئة.
    pub fn with_category(mut self, category: AuditCategory) -> Self {
        self.category = Some(category);
        self
    }

    /// فلترة الناجحة فقط.
    pub fn success_only(mut self) -> Self {
        self.success_only = Some(true);
        self
    }

    /// فلترة الفاشلة فقط.
    pub fn failures_only(mut self) -> Self {
        self.success_only = Some(false);
        self
    }

    /// فلترة من طابع زمني.
    pub fn from(mut self, ts: i64) -> Self {
        self.from_timestamp = Some(ts);
        self
    }

    /// فلترة حتى طابع زمني.
    pub fn to(mut self, ts: i64) -> Self {
        self.to_timestamp = Some(ts);
        self
    }

    /// تعيين الحد.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// تعيين الإزاحة.
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    /// تعيين الترتيب.
    pub fn with_sort(mut self, sort: AuditSort) -> Self {
        self.sort = sort;
        self
    }

    /// التحقق من مطابقة مدخل للفلتر.
    pub fn matches(&self, entry: &crate::entry::AuditEntry) -> bool {
        if let Some(actor) = &self.actor {
            if entry.actor != *actor {
                return false;
            }
        }
        if let Some(action) = &self.action {
            if entry.action != *action {
                return false;
            }
        }
        if let Some(target) = &self.target {
            if entry.target != *target {
                return false;
            }
        }
        if let Some(cat) = self.category {
            if entry.category != cat {
                return false;
            }
        }
        if let Some(success) = self.success_only {
            if entry.success != success {
                return false;
            }
        }
        if let Some(from) = self.from_timestamp {
            if entry.timestamp < from {
                return false;
            }
        }
        if let Some(to) = self.to_timestamp {
            if entry.timestamp > to {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::AuditEntry;

    fn make_entry(actor: &str, action: &str, ts: i64, success: bool) -> AuditEntry {
        AuditEntry::new(actor, action, "target")
            .with_timestamp(ts)
            .with_success(success)
    }

    #[test]
    fn empty_filter_matches_all() {
        let f = AuditFilter::new();
        let e = make_entry("u", "a", 100, true);
        assert!(f.matches(&e));
    }

    #[test]
    fn filter_by_actor() {
        let f = AuditFilter::new().with_actor("alice");
        assert!(f.matches(&make_entry("alice", "x", 0, true)));
        assert!(!f.matches(&make_entry("bob", "x", 0, true)));
    }

    #[test]
    fn filter_by_action() {
        let f = AuditFilter::new().with_action("login");
        assert!(f.matches(&make_entry("u", "login", 0, true)));
        assert!(!f.matches(&make_entry("u", "logout", 0, true)));
    }

    #[test]
    fn filter_by_category() {
        let f = AuditFilter::new().with_category(AuditCategory::Auth);
        let auth_entry = AuditEntry::new("u", "a", "t")
            .with_category(AuditCategory::Auth)
            .with_timestamp(0);
        let billing_entry = AuditEntry::new("u", "a", "t")
            .with_category(AuditCategory::Billing)
            .with_timestamp(0);
        assert!(f.matches(&auth_entry));
        assert!(!f.matches(&billing_entry));
    }

    #[test]
    fn filter_success_only() {
        let f = AuditFilter::new().success_only();
        assert!(f.matches(&make_entry("u", "a", 0, true)));
        assert!(!f.matches(&make_entry("u", "a", 0, false)));
    }

    #[test]
    fn filter_failures_only() {
        let f = AuditFilter::new().failures_only();
        assert!(!f.matches(&make_entry("u", "a", 0, true)));
        assert!(f.matches(&make_entry("u", "a", 0, false)));
    }

    #[test]
    fn filter_by_time_range() {
        let f = AuditFilter::new().from(100).to(200);
        assert!(f.matches(&make_entry("u", "a", 150, true)));
        assert!(!f.matches(&make_entry("u", "a", 50, true)));
        assert!(!f.matches(&make_entry("u", "a", 250, true)));
    }

    #[test]
    fn filter_from_only() {
        let f = AuditFilter::new().from(100);
        assert!(f.matches(&make_entry("u", "a", 100, true)));
        assert!(f.matches(&make_entry("u", "a", 200, true)));
        assert!(!f.matches(&make_entry("u", "a", 50, true)));
    }

    #[test]
    fn filter_to_only() {
        let f = AuditFilter::new().to(100);
        assert!(f.matches(&make_entry("u", "a", 50, true)));
        assert!(f.matches(&make_entry("u", "a", 100, true)));
        assert!(!f.matches(&make_entry("u", "a", 150, true)));
    }

    #[test]
    fn combined_filters() {
        let f = AuditFilter::new()
            .with_actor("alice")
            .with_action("login")
            .with_category(AuditCategory::Auth)
            .success_only()
            .from(100);

        let matching = AuditEntry::new("alice", "login", "session")
            .with_category(AuditCategory::Auth)
            .with_success(true)
            .with_timestamp(150);
        assert!(f.matches(&matching));

        let not_matching = AuditEntry::new("bob", "login", "session")
            .with_category(AuditCategory::Auth)
            .with_success(true)
            .with_timestamp(150);
        assert!(!f.matches(&not_matching));
    }

    #[test]
    fn serde_roundtrip() {
        let f = AuditFilter::new()
            .with_actor("alice")
            .with_category(AuditCategory::Auth)
            .with_limit(50);
        let json = serde_json::to_string(&f).unwrap();
        let back: AuditFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(back.actor, Some("alice".into()));
        assert_eq!(back.category, Some(AuditCategory::Auth));
        assert_eq!(back.limit, Some(50));
    }

    #[test]
    fn sort_default() {
        let f = AuditFilter::new();
        assert_eq!(f.sort, AuditSort::NewestFirst);
    }
}
