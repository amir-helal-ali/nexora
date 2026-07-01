//! Notification store — in-memory, thread-safe.

use crate::types::{Notification, NotificationId, NotificationSeverity};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Error from notification operations.
#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    /// Notification not found.
    #[error("notification not found: {0}")]
    NotFound(NotificationId),
}

/// The notification store. Thread-safe.
pub struct NotificationStore {
    notifications: RwLock<Vec<Notification>>,
    by_user: RwLock<HashMap<String, Vec<usize>>>,
    event_bus: Option<Arc<nexora_core::EventBus>>,
}

impl fmt::Debug for NotificationStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NotificationStore")
            .field("count", &self.notifications.read().len())
            .finish()
    }
}

impl Default for NotificationStore {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationStore {
    /// Construct an empty store.
    pub fn new() -> Self {
        Self {
            notifications: RwLock::new(Vec::new()),
            by_user: RwLock::new(HashMap::new()),
            event_bus: None,
        }
    }

    /// Attach an EventBus.
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Total notification count.
    pub fn count(&self) -> usize {
        self.notifications.read().len()
    }

    /// Create a notification for a user.
    pub fn create(&self, notification: Notification) -> Notification {
        let user_id = notification.user_id.clone();
        let id = notification.id.clone();
        let mut notifs = self.notifications.write();
        let idx = notifs.len();
        notifs.push(notification.clone());
        drop(notifs);
        self.by_user
            .write()
            .entry(user_id)
            .or_default()
            .push(idx);
        if let Some(bus) = &self.event_bus {
            bus.publish("notification.created", id);
        }
        notification
    }

    /// Get a notification by ID.
    pub fn get(&self, id: &str) -> Option<Notification> {
        self.notifications.read().iter().find(|n| n.id == id).cloned()
    }

    /// List notifications for a user (newest first).
    pub fn list_for_user(&self, user_id: &str) -> Vec<Notification> {
        let by_user = self.by_user.read();
        let notifs = self.notifications.read();
        by_user
            .get(user_id)
            .cloned()
            .unwrap_or_default()
            .iter()
            .rev()
            .filter_map(|&idx| notifs.get(idx).cloned())
            .collect()
    }

    /// Count unread notifications for a user.
    pub fn unread_count(&self, user_id: &str) -> usize {
        self.list_for_user(user_id).iter().filter(|n| !n.read).count()
    }

    /// Mark a notification as read.
    pub fn mark_read(&self, id: &str) -> Result<Notification, NotificationError> {
        let mut notifs = self.notifications.write();
        let notif = notifs
            .iter_mut()
            .find(|n| n.id == id)
            .ok_or_else(|| NotificationError::NotFound(id.to_string()))?;
        notif.read = true;
        Ok(notif.clone())
    }

    /// Mark all notifications as read for a user.
    pub fn mark_all_read(&self, user_id: &str) -> usize {
        let by_user = self.by_user.read();
        let indices = by_user.get(user_id).cloned().unwrap_or_default();
        drop(by_user);
        let mut notifs = self.notifications.write();
        let mut count = 0;
        for idx in &indices {
            if let Some(n) = notifs.get_mut(*idx) {
                if !n.read {
                    n.read = true;
                    count += 1;
                }
            }
        }
        count
    }

    /// Delete a notification.
    pub fn delete(&self, id: &str) -> Result<(), NotificationError> {
        let mut notifs = self.notifications.write();
        let pos = notifs.iter().position(|n| n.id == id)
            .ok_or_else(|| NotificationError::NotFound(id.to_string()))?;
        let user_id = notifs[pos].user_id.clone();
        notifs.remove(pos);
        drop(notifs);
        // Rebuild index (indices shifted).
        let all = self.notifications.read();
        let mut new_map: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, n) in all.iter().enumerate() {
            new_map.entry(n.user_id.clone()).or_default().push(i);
        }
        *self.by_user.write() = new_map;
        let _ = user_id; // suppress unused warning
        Ok(())
    }

    /// Delete all read notifications for a user.
    pub fn delete_read(&self, user_id: &str) -> usize {
        let by_user = self.by_user.read();
        let indices = by_user.get(user_id).cloned().unwrap_or_default();
        drop(by_user);
        let read_ids: Vec<String> = {
            let notifs = self.notifications.read();
            indices
                .iter()
                .filter_map(|&idx| {
                    notifs.get(idx).filter(|n| n.read).map(|n| n.id.clone())
                })
                .collect()
        };
        let count = read_ids.len();
        for id in &read_ids {
            let _ = self.delete(id);
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> NotificationStore {
        let bus = Arc::new(nexora_core::EventBus::new());
        NotificationStore::new().with_event_bus(bus)
    }

    #[test]
    fn create_and_list() {
        let store = setup();
        let n = Notification::new("u1", "Welcome", "Hello!", NotificationSeverity::Info);
        store.create(n);
        let list = store.list_for_user("u1");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Welcome");
    }

    #[test]
    fn unread_count() {
        let store = setup();
        store.create(Notification::new("u1", "A", "a", NotificationSeverity::Info));
        store.create(Notification::new("u1", "B", "b", NotificationSeverity::Warning));
        store.create(Notification::new("u2", "C", "c", NotificationSeverity::Error));
        assert_eq!(store.unread_count("u1"), 2);
        assert_eq!(store.unread_count("u2"), 1);
    }

    #[test]
    fn mark_read() {
        let store = setup();
        let n = store.create(Notification::new("u1", "A", "a", NotificationSeverity::Info));
        assert_eq!(store.unread_count("u1"), 1);
        store.mark_read(&n.id).unwrap();
        assert_eq!(store.unread_count("u1"), 0);
    }

    #[test]
    fn mark_all_read() {
        let store = setup();
        store.create(Notification::new("u1", "A", "a", NotificationSeverity::Info));
        store.create(Notification::new("u1", "B", "b", NotificationSeverity::Warning));
        let count = store.mark_all_read("u1");
        assert_eq!(count, 2);
        assert_eq!(store.unread_count("u1"), 0);
    }

    #[test]
    fn delete_notification() {
        let store = setup();
        let n = store.create(Notification::new("u1", "A", "a", NotificationSeverity::Info));
        store.delete(&n.id).unwrap();
        assert_eq!(store.list_for_user("u1").len(), 0);
    }

    #[test]
    fn delete_read_only_removes_read() {
        let store = setup();
        let n1 = store.create(Notification::new("u1", "A", "a", NotificationSeverity::Info));
        store.create(Notification::new("u1", "B", "b", NotificationSeverity::Warning));
        store.mark_read(&n1.id).unwrap();
        let deleted = store.delete_read("u1");
        assert_eq!(deleted, 1);
        assert_eq!(store.list_for_user("u1").len(), 1);
        assert_eq!(store.list_for_user("u1")[0].title, "B");
    }

    #[test]
    fn list_newest_first() {
        let store = setup();
        store.create(Notification::new("u1", "First", "1", NotificationSeverity::Info));
        std::thread::sleep(std::time::Duration::from_millis(1));
        store.create(Notification::new("u1", "Second", "2", NotificationSeverity::Info));
        let list = store.list_for_user("u1");
        assert_eq!(list[0].title, "Second");
        assert_eq!(list[1].title, "First");
    }

    #[test]
    fn events_emitted() {
        let bus = Arc::new(nexora_core::EventBus::new());
        let store = NotificationStore::new().with_event_bus(bus.clone());
        store.create(Notification::new("u1", "A", "a", NotificationSeverity::Info));
        let events = bus.replay_filtered(0, "notification.");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "notification.created");
    }
}
