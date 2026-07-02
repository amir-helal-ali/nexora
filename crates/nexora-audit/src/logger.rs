//! مسجّل التدقيق — يخزّن ويستعلم عن المدخلات.

use crate::entry::{AuditEntry, AuditEntryId};
use crate::filter::{AuditFilter, AuditSort};
use parking_lot::RwLock;
use std::collections::HashMap;
use time::OffsetDateTime;

/// نتيجة استعلام التدقيق.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditQueryResult {
    /// المدخلات المطابقة.
    pub entries: Vec<AuditEntry>,
    /// إجمالي المدخلات المطابقة (قبل الترقيم).
    pub total: usize,
    /// الحد المطبّق.
    pub limit: usize,
    /// الإزاحة المطبّقة.
    pub offset: usize,
}

/// مسجّل التدقيق — يخزّن المدخلات في الذاكرة.
///
/// في الإنتاج، يُستبدل بمخزن PostgreSQL (PgAuditStore) أو مشابه.
pub struct AuditLogger {
    /// المدخلات مخزّنة في Vec (مرتبة زمنياً).
    entries: RwLock<Vec<AuditEntry>>,
    /// فهرس بالمعرّف للوصول السريع.
    by_id: RwLock<HashMap<AuditEntryId, usize>>,
    /// حد أقصى للمدخلات المخزّنة (لمنع النمو غير المحدود).
    max_entries: usize,
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new(100_000)
    }
}

impl AuditLogger {
    /// إنشاء مسجّل بحد أقصى محدد.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            by_id: RwLock::new(HashMap::new()),
            max_entries,
        }
    }

    /// تسجيل مدخل جديد.
    pub fn log(&self, entry: AuditEntry) -> AuditEntryId {
        let id = entry.id.clone();
        let mut entries = self.entries.write();
        let mut by_id = self.by_id.write();
        let idx = entries.len();
        entries.push(entry);
        by_id.insert(id.clone(), idx);

        // إزالة أقدم مدخل عند تجاوز الحد.
        if entries.len() > self.max_entries {
            let removed = entries.remove(0);
            by_id.remove(&removed.id);
            // تحديث الفهارس (نقص 1 من كل idx > 0).
            let mut new_by_id = HashMap::new();
            for (i, e) in entries.iter().enumerate() {
                new_by_id.insert(e.id.clone(), i);
            }
            *by_id = new_by_id;
        }
        id
    }

    /// الحصول على مدخل بالمعرّف.
    pub fn get(&self, id: &AuditEntryId) -> Option<AuditEntry> {
        let entries = self.entries.read();
        let by_id = self.by_id.read();
        by_id.get(id).and_then(|&idx| entries.get(idx).cloned())
    }

    /// الاستعلام عن المدخلات بفلتر.
    pub fn query(&self, filter: &AuditFilter) -> AuditQueryResult {
        let entries = self.entries.read();
        let limit = filter.limit.unwrap_or(100);
        let offset = filter.offset.unwrap_or(0);

        // فلترة.
        let mut matching: Vec<&AuditEntry> = entries.iter().filter(|e| filter.matches(e)).collect();

        // ترتيب.
        match filter.sort {
            AuditSort::NewestFirst => matching.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)),
            AuditSort::OldestFirst => matching.sort_by(|a, b| a.timestamp.cmp(&b.timestamp)),
        }

        let total = matching.len();
        // ترقيم.
        let paged: Vec<AuditEntry> = matching
            .into_iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();

        AuditQueryResult {
            entries: paged,
            total,
            limit,
            offset,
        }
    }

    /// عدد كل المدخلات.
    pub fn count(&self) -> usize {
        self.entries.read().len()
    }

    /// عدد المدخلات لفاعل محدد.
    pub fn count_for_actor(&self, actor: &str) -> usize {
        self.entries.read().iter().filter(|e| e.actor == actor).count()
    }

    /// عدد المدخلات لفئة محددة.
    pub fn count_for_category(&self, category: crate::category::AuditCategory) -> usize {
        self.entries
            .read()
            .iter()
            .filter(|e| e.category == category)
            .count()
    }

    /// عدّ النتائج حسب الفئة.
    pub fn count_by_category(&self) -> HashMap<crate::category::AuditCategory, usize> {
        let entries = self.entries.read();
        let mut counts = HashMap::new();
        for e in entries.iter() {
            *counts.entry(e.category).or_insert(0) += 1;
        }
        counts
    }

    /// إفراغ السجل.
    pub fn clear(&self) {
        self.entries.write().clear();
        self.by_id.write().clear();
    }

    /// إحصائيات سريعة.
    pub fn stats(&self) -> AuditStats {
        let entries = self.entries.read();
        let total = entries.len();
        let success = entries.iter().filter(|e| e.success).count();
        let failure = total - success;
        let categories = self.count_by_category();
        AuditStats {
            total,
            success,
            failure,
            categories,
        }
    }
}

/// إحصائيات سجل التدقيق.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditStats {
    pub total: usize,
    pub success: usize,
    pub failure: usize,
    pub categories: HashMap<crate::category::AuditCategory, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::AuditCategory;
    use crate::entry::AuditEntry;
    use crate::filter::AuditFilter;

    fn make_entry(actor: &str, action: &str, ts: i64, success: bool) -> AuditEntry {
        AuditEntry::new(actor, action, "target")
            .with_timestamp(ts)
            .with_success(success)
            .with_category(AuditCategory::Auth)
    }

    #[test]
    fn log_and_get() {
        let logger = AuditLogger::default();
        let e = make_entry("alice", "login", 100, true);
        let id = logger.log(e);
        assert_eq!(logger.count(), 1);
        assert!(logger.get(&id).is_some());
    }

    #[test]
    fn query_by_actor() {
        let logger = AuditLogger::default();
        logger.log(make_entry("alice", "login", 100, true));
        logger.log(make_entry("bob", "login", 200, true));
        logger.log(make_entry("alice", "logout", 300, true));

        let result = logger.query(&AuditFilter::new().with_actor("alice"));
        assert_eq!(result.total, 2);
        assert_eq!(result.entries.len(), 2);
    }

    #[test]
    fn query_with_limit() {
        let logger = AuditLogger::default();
        for i in 0..50 {
            logger.log(make_entry("u", "a", i, true));
        }
        let result = logger.query(&AuditFilter::new().with_limit(10));
        assert_eq!(result.total, 50);
        assert_eq!(result.entries.len(), 10);
    }

    #[test]
    fn query_with_offset() {
        let logger = AuditLogger::default();
        for i in 0..20 {
            logger.log(make_entry("u", "a", i, true));
        }
        let result = logger.query(&AuditFilter::new().with_limit(10).with_offset(10));
        assert_eq!(result.total, 20);
        assert_eq!(result.entries.len(), 10);
    }

    #[test]
    fn query_newest_first() {
        let logger = AuditLogger::default();
        logger.log(make_entry("u", "a", 100, true));
        logger.log(make_entry("u", "a", 300, true));
        logger.log(make_entry("u", "a", 200, true));

        let result = logger.query(&AuditFilter::new().with_limit(3));
        assert_eq!(result.entries[0].timestamp, 300);
        assert_eq!(result.entries[1].timestamp, 200);
        assert_eq!(result.entries[2].timestamp, 100);
    }

    #[test]
    fn query_oldest_first() {
        let logger = AuditLogger::default();
        logger.log(make_entry("u", "a", 300, true));
        logger.log(make_entry("u", "a", 100, true));
        logger.log(make_entry("u", "a", 200, true));

        let result = logger.query(
            &AuditFilter::new()
                .with_limit(3)
                .with_sort(crate::filter::AuditSort::OldestFirst),
        );
        assert_eq!(result.entries[0].timestamp, 100);
        assert_eq!(result.entries[1].timestamp, 200);
        assert_eq!(result.entries[2].timestamp, 300);
    }

    #[test]
    fn query_failures_only() {
        let logger = AuditLogger::default();
        logger.log(make_entry("u", "a", 100, true));
        logger.log(make_entry("u", "a", 200, false));
        logger.log(make_entry("u", "a", 300, true));

        let result = logger.query(&AuditFilter::new().failures_only());
        assert_eq!(result.total, 1);
        assert!(!result.entries[0].success);
    }

    #[test]
    fn query_by_category() {
        let logger = AuditLogger::default();
        logger.log(AuditEntry::new("u", "a", "t").with_category(AuditCategory::Auth));
        logger.log(AuditEntry::new("u", "a", "t").with_category(AuditCategory::Billing));
        logger.log(AuditEntry::new("u", "a", "t").with_category(AuditCategory::Auth));

        let result = logger.query(&AuditFilter::new().with_category(AuditCategory::Auth));
        assert_eq!(result.total, 2);
    }

    #[test]
    fn count_for_actor() {
        let logger = AuditLogger::default();
        logger.log(make_entry("alice", "a", 0, true));
        logger.log(make_entry("alice", "b", 0, true));
        logger.log(make_entry("bob", "c", 0, true));
        assert_eq!(logger.count_for_actor("alice"), 2);
        assert_eq!(logger.count_for_actor("bob"), 1);
        assert_eq!(logger.count_for_actor("nobody"), 0);
    }

    #[test]
    fn count_by_category() {
        let logger = AuditLogger::default();
        logger.log(AuditEntry::new("u", "a", "t").with_category(AuditCategory::Auth));
        logger.log(AuditEntry::new("u", "a", "t").with_category(AuditCategory::Auth));
        logger.log(AuditEntry::new("u", "a", "t").with_category(AuditCategory::Billing));

        let counts = logger.count_by_category();
        assert_eq!(counts.get(&AuditCategory::Auth), Some(&2));
        assert_eq!(counts.get(&AuditCategory::Billing), Some(&1));
    }

    #[test]
    fn clear_empties_log() {
        let logger = AuditLogger::default();
        logger.log(make_entry("u", "a", 0, true));
        assert_eq!(logger.count(), 1);
        logger.clear();
        assert_eq!(logger.count(), 0);
    }

    #[test]
    fn max_entries_eviction() {
        let logger = AuditLogger::new(3);
        let id1 = logger.log(make_entry("u", "a", 100, true));
        logger.log(make_entry("u", "a", 200, true));
        logger.log(make_entry("u", "a", 300, true));
        logger.log(make_entry("u", "a", 400, true));

        assert_eq!(logger.count(), 3);
        // أقدم مدخل يجب أن يُزال.
        assert!(logger.get(&id1).is_none());
    }

    #[test]
    fn stats() {
        let logger = AuditLogger::default();
        logger.log(make_entry("u", "a", 0, true));
        logger.log(make_entry("u", "a", 0, true));
        logger.log(make_entry("u", "a", 0, false));
        let stats = logger.stats();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.success, 2);
        assert_eq!(stats.failure, 1);
    }

    #[test]
    fn query_time_range() {
        let logger = AuditLogger::default();
        logger.log(make_entry("u", "a", 100, true));
        logger.log(make_entry("u", "a", 200, true));
        logger.log(make_entry("u", "a", 300, true));

        let result = logger.query(&AuditFilter::new().from(150).to(250));
        assert_eq!(result.total, 1);
        assert_eq!(result.entries[0].timestamp, 200);
    }

    #[test]
    fn query_empty_filter_returns_all() {
        let logger = AuditLogger::default();
        logger.log(make_entry("u", "a", 0, true));
        logger.log(make_entry("u", "a", 0, true));
        let result = logger.query(&AuditFilter::new());
        assert_eq!(result.total, 2);
    }
}
