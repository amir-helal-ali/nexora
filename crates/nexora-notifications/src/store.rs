//! In-memory in-app notification store.
//!
//! In-app notifications are short-lived notifications shown in the Nexora
//! dashboard. They're fetched via SSE and don't require an external service.

use crate::error::{NotificationError, NotificationResult};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use time::OffsetDateTime;

/// A stored in-app notification (with read state).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InAppNotification {
    /// Notification ID.
    pub id: String,
    /// User ID this notification belongs to.
    pub user_id: String,
    /// Title.
    pub title: String,
    /// Body.
    pub body: String,
    /// Optional action URL.
    pub action_url: Option<String>,
    /// When it was created (unix nanos).
    pub created_at: i64,
    /// When it was read (unix nanos), or None if unread.
    #[serde(default)]
    pub read_at: Option<i64>,
}

impl InAppNotification {
    /// Whether this notification has been read.
    pub fn is_read(&self) -> bool {
        self.read_at.is_some()
    }
}

/// In-memory in-app notification store. Production deployments with
/// multiple gateway instances should use a shared cache (Redis).
pub struct InAppStore {
    by_user: RwLock<HashMap<String, Vec<InAppNotification>>>,
    /// Cap per-user to prevent unbounded growth.
    per_user_cap: usize,
}

impl Default for InAppStore {
    fn default() -> Self {
        Self::new(100)
    }
}

impl InAppStore {
    /// Construct with a per-user cap (oldest are evicted when cap exceeded).
    pub fn new(per_user_cap: usize) -> Self {
        Self {
            by_user: RwLock::new(HashMap::new()),
            per_user_cap,
        }
    }

    /// Add a notification for a user.
    pub fn add(
        &self,
        user_id: &str,
        title: &str,
        body: &str,
        action_url: Option<String>,
    ) -> NotificationResult<InAppNotification> {
        if user_id.is_empty() {
            return Err(NotificationError::InvalidRecipient("user_id is empty".into()));
        }
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let n = InAppNotification {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            action_url,
            created_at: now,
            read_at: None,
        };
        let mut store = self.by_user.write();
        let list = store.entry(user_id.to_string()).or_insert_with(Vec::new);
        list.push(n.clone());
        // Evict oldest if over cap.
        while list.len() > self.per_user_cap {
            list.remove(0);
        }
        Ok(n)
    }

    /// List notifications for a user (newest first).
    pub fn list(&self, user_id: &str, limit: usize) -> Vec<InAppNotification> {
        let store = self.by_user.read();
        match store.get(user_id) {
            Some(list) => {
                let mut v: Vec<_> = list.iter().rev().take(limit).cloned().collect();
                v
            }
            None => Vec::new(),
        }
    }

    /// List unread notifications for a user.
    pub fn list_unread(&self, user_id: &str) -> Vec<InAppNotification> {
        let store = self.by_user.read();
        match store.get(user_id) {
            Some(list) => list.iter().filter(|n| !n.is_read()).cloned().collect(),
            None => Vec::new(),
        }
    }

    /// Mark a notification as read. Returns true if found.
    pub fn mark_read(&self, user_id: &str, notification_id: &str) -> bool {
        let mut store = self.by_user.write();
        if let Some(list) = store.get_mut(user_id) {
            for n in list.iter_mut() {
                if n.id == notification_id {
                    n.read_at = Some(OffsetDateTime::now_utc().unix_timestamp_nanos() as i64);
                    return true;
                }
            }
        }
        false
    }

    /// Mark all notifications as read for a user.
    pub fn mark_all_read(&self, user_id: &str) -> usize {
        let mut store = self.by_user.write();
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let mut count = 0;
        if let Some(list) = store.get_mut(user_id) {
            for n in list.iter_mut() {
                if n.read_at.is_none() {
                    n.read_at = Some(now);
                    count += 1;
                }
            }
        }
        count
    }

    /// Delete a notification.
    pub fn delete(&self, user_id: &str, notification_id: &str) -> bool {
        let mut store = self.by_user.write();
        if let Some(list) = store.get_mut(user_id) {
            let before = list.len();
            list.retain(|n| n.id != notification_id);
            return list.len() != before;
        }
        false
    }

    /// Count total notifications for a user.
    pub fn count(&self, user_id: &str) -> usize {
        self.by_user.read().get(user_id).map(|v| v.len()).unwrap_or(0)
    }

    /// Count unread notifications for a user.
    pub fn unread_count(&self, user_id: &str) -> usize {
        self.by_user
            .read()
            .get(user_id)
            .map(|v| v.iter().filter(|n| !n.is_read()).count())
            .unwrap_or(0)
    }

    /// Clear all notifications for a user.
    pub fn clear(&self, user_id: &str) -> usize {
        let mut store = self.by_user.write();
        store.remove(user_id).map(|v| v.len()).unwrap_or(0)
    }
}

/// Adapter that implements `Channel` by writing to the InAppStore.
pub struct InAppChannel {
    store: std::sync::Arc<InAppStore>,
}

impl InAppChannel {
    pub fn new(store: std::sync::Arc<InAppStore>) -> Self {
        Self { store }
    }

    /// Access the underlying store.
    pub fn store(&self) -> &std::sync::Arc<InAppStore> {
        &self.store
    }
}

#[async_trait::async_trait]
impl crate::channel::Channel for InAppChannel {
    fn kind(&self) -> crate::channel::ChannelKind {
        crate::channel::ChannelKind::InApp
    }

    fn name(&self) -> &str {
        "in_app"
    }

    async fn deliver(&self, n: &crate::message::Notification) -> NotificationResult<()> {
        self.store.add(
            &n.user_id,
            &n.payload.title,
            &n.payload.body,
            n.payload.action_url.clone(),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_list() {
        let s = InAppStore::default();
        let n1 = s.add("u1", "T1", "B1", None).unwrap();
        let n2 = s.add("u1", "T2", "B2", None).unwrap();
        let list = s.list("u1", 10);
        assert_eq!(list.len(), 2);
        // Newest first.
        assert_eq!(list[0].id, n2.id);
        assert_eq!(list[1].id, n1.id);
    }

    #[test]
    fn list_for_unknown_user_returns_empty() {
        let s = InAppStore::default();
        assert!(s.list("nobody", 10).is_empty());
    }

    #[test]
    fn mark_read() {
        let s = InAppStore::default();
        let n = s.add("u1", "T", "B", None).unwrap();
        assert!(!n.is_read());
        assert_eq!(s.unread_count("u1"), 1);
        assert!(s.mark_read("u1", &n.id));
        assert_eq!(s.unread_count("u1"), 0);
        assert!(s.list("u1", 10)[0].is_read());
    }

    #[test]
    fn mark_all_read() {
        let s = InAppStore::default();
        s.add("u1", "T1", "B1", None).unwrap();
        s.add("u1", "T2", "B2", None).unwrap();
        s.add("u1", "T3", "B3", None).unwrap();
        assert_eq!(s.mark_all_read("u1"), 3);
        assert_eq!(s.unread_count("u1"), 0);
        // Idempotent.
        assert_eq!(s.mark_all_read("u1"), 0);
    }

    #[test]
    fn delete_notification() {
        let s = InAppStore::default();
        let n = s.add("u1", "T", "B", None).unwrap();
        assert!(s.delete("u1", &n.id));
        assert!(!s.delete("u1", &n.id));
        assert_eq!(s.count("u1"), 0);
    }

    #[test]
    fn cap_evicts_oldest() {
        let s = InAppStore::new(3);
        s.add("u1", "T1", "B", None).unwrap();
        s.add("u1", "T2", "B", None).unwrap();
        s.add("u1", "T3", "B", None).unwrap();
        s.add("u1", "T4", "B", None).unwrap();
        let list = s.list("u1", 10);
        assert_eq!(list.len(), 3);
        // T1 should be evicted (oldest).
        assert!(list.iter().all(|n| n.title != "T1"));
    }

    #[test]
    fn clear_all() {
        let s = InAppStore::default();
        s.add("u1", "T", "B", None).unwrap();
        s.add("u1", "T2", "B", None).unwrap();
        assert_eq!(s.clear("u1"), 2);
        assert_eq!(s.count("u1"), 0);
    }

    #[test]
    fn empty_user_id_rejected() {
        let s = InAppStore::default();
        assert!(s.add("", "T", "B", None).is_err());
    }

    #[tokio::test]
    async fn in_app_channel_delivers() {
        use crate::channel::Channel;
        use crate::message::{Notification, NotificationPayload};
        let store = std::sync::Arc::new(InAppStore::default());
        let ch = InAppChannel::new(store.clone());
        let n = Notification::new("u1", "in_app", "in_app", NotificationPayload::new("T", "B"));
        ch.deliver(&n).await.unwrap();
        assert_eq!(store.count("u1"), 1);
    }
}
