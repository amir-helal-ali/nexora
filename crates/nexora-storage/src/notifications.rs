//! SQLite-backed notification store — persists user notifications.

use crate::{Database, StorageError};
use nexora_notifications::types::{Notification, NotificationSeverity};
use std::sync::Arc;

/// SQLite-backed notification store.
pub struct SqliteNotificationStore {
    db: Database,
    event_bus: Option<Arc<nexora_core::EventBus>>,
}

impl std::fmt::Debug for SqliteNotificationStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteNotificationStore")
            .field("db", &self.db)
            .finish()
    }
}

impl SqliteNotificationStore {
    /// Construct a new store.
    pub fn new(db: Database) -> Self {
        Self { db, event_bus: None }
    }

    /// Attach an EventBus.
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Save a notification (insert or replace).
    pub fn save(&self, n: &Notification) -> Result<(), StorageError> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO notifications
                 (id, user_id, title, body, severity, read, created_at, link, icon)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    n.id,
                    n.user_id,
                    n.title,
                    n.body,
                    n.severity.to_string(),
                    if n.read { 1 } else { 0 },
                    n.created_at,
                    n.link,
                    n.icon,
                ],
            )?;
            Ok(())
        })
    }

    /// Mark a notification as read.
    pub fn mark_read(&self, id: &str) -> Result<(), StorageError> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE notifications SET read = 1 WHERE id = ?1",
                rusqlite::params![id],
            )?;
            Ok(())
        })
    }

    /// Mark all notifications as read for a user.
    pub fn mark_all_read(&self, user_id: &str) -> Result<usize, StorageError> {
        self.db.with_conn(|conn| {
            let count = conn.execute(
                "UPDATE notifications SET read = 1 WHERE user_id = ?1 AND read = 0",
                rusqlite::params![user_id],
            )?;
            Ok(count)
        })
    }

    /// Delete a notification.
    pub fn delete(&self, id: &str) -> Result<(), StorageError> {
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM notifications WHERE id = ?1", rusqlite::params![id])?;
            Ok(())
        })
    }

    /// Delete all read notifications for a user.
    pub fn delete_read(&self, user_id: &str) -> Result<usize, StorageError> {
        self.db.with_conn(|conn| {
            let count = conn.execute(
                "DELETE FROM notifications WHERE user_id = ?1 AND read = 1",
                rusqlite::params![user_id],
            )?;
            Ok(count)
        })
    }

    /// Count all notifications.
    pub fn count(&self) -> Result<i64, StorageError> {
        self.db.with_conn(|conn| {
            Ok(conn.query_row("SELECT COUNT(*) FROM notifications", [], |row| row.get(0))?)
        })
    }

    /// Count unread for a user.
    pub fn unread_count(&self, user_id: &str) -> Result<i64, StorageError> {
        self.db.with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND read = 0",
                rusqlite::params![user_id],
                |row| row.get(0),
            )?)
        })
    }

    /// Load notifications for a user (newest first).
    pub fn load_for_user(&self, user_id: &str, limit: usize) -> Result<Vec<Notification>, StorageError> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, user_id, title, body, severity, read, created_at, link, icon
                 FROM notifications
                 WHERE user_id = ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![user_id, limit as i64], |row| {
                let id: String = row.get(0)?;
                let uid: String = row.get(1)?;
                let title: String = row.get(2)?;
                let body: String = row.get(3)?;
                let severity_str: String = row.get(4)?;
                let read: i64 = row.get(5)?;
                let created_at: i64 = row.get(6)?;
                let link: Option<String> = row.get(7)?;
                let icon: Option<String> = row.get(8)?;

                let severity = match severity_str.as_str() {
                    "success" => NotificationSeverity::Success,
                    "warning" => NotificationSeverity::Warning,
                    "error" => NotificationSeverity::Error,
                    _ => NotificationSeverity::Info,
                };

                Ok(Notification {
                    id,
                    user_id: uid,
                    title,
                    body,
                    severity,
                    read: read != 0,
                    created_at,
                    link,
                    icon,
                })
            })?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok(result)
        })
    }

    /// Load ALL notifications (for admin/observability).
    pub fn load_all(&self, limit: usize) -> Result<Vec<Notification>, StorageError> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, user_id, title, body, severity, read, created_at, link, icon
                 FROM notifications
                 ORDER BY created_at DESC
                 LIMIT ?1",
            )?;
            let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
                let id: String = row.get(0)?;
                let uid: String = row.get(1)?;
                let title: String = row.get(2)?;
                let body: String = row.get(3)?;
                let severity_str: String = row.get(4)?;
                let read: i64 = row.get(5)?;
                let created_at: i64 = row.get(6)?;
                let link: Option<String> = row.get(7)?;
                let icon: Option<String> = row.get(8)?;

                let severity = match severity_str.as_str() {
                    "success" => NotificationSeverity::Success,
                    "warning" => NotificationSeverity::Warning,
                    "error" => NotificationSeverity::Error,
                    _ => NotificationSeverity::Info,
                };

                Ok(Notification {
                    id,
                    user_id: uid,
                    title,
                    body,
                    severity,
                    read: read != 0,
                    created_at,
                    link,
                    icon,
                })
            })?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok(result)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> SqliteNotificationStore {
        let db = Database::open_in_memory().unwrap();
        SqliteNotificationStore::new(db)
    }

    fn sample_notif(user_id: &str, title: &str) -> Notification {
        Notification::new(user_id, title, "body text", NotificationSeverity::Info)
    }

    #[test]
    fn save_and_count() {
        let store = setup();
        assert_eq!(store.count().unwrap(), 0);
        store.save(&sample_notif("u1", "A")).unwrap();
        store.save(&sample_notif("u1", "B")).unwrap();
        store.save(&sample_notif("u2", "C")).unwrap();
        assert_eq!(store.count().unwrap(), 3);
    }

    #[test]
    fn unread_count_per_user() {
        let store = setup();
        store.save(&sample_notif("u1", "A")).unwrap();
        store.save(&sample_notif("u1", "B")).unwrap();
        store.save(&sample_notif("u2", "C")).unwrap();
        assert_eq!(store.unread_count("u1").unwrap(), 2);
        assert_eq!(store.unread_count("u2").unwrap(), 1);
    }

    #[test]
    fn mark_read_single() {
        let store = setup();
        let n = sample_notif("u1", "A");
        store.save(&n).unwrap();
        assert_eq!(store.unread_count("u1").unwrap(), 1);
        store.mark_read(&n.id).unwrap();
        assert_eq!(store.unread_count("u1").unwrap(), 0);
    }

    #[test]
    fn mark_all_read() {
        let store = setup();
        store.save(&sample_notif("u1", "A")).unwrap();
        store.save(&sample_notif("u1", "B")).unwrap();
        let count = store.mark_all_read("u1").unwrap();
        assert_eq!(count, 2);
        assert_eq!(store.unread_count("u1").unwrap(), 0);
    }

    #[test]
    fn delete_single() {
        let store = setup();
        let n = sample_notif("u1", "A");
        store.save(&n).unwrap();
        store.delete(&n.id).unwrap();
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn delete_read_only() {
        let store = setup();
        let n1 = sample_notif("u1", "A");
        store.save(&n1).unwrap();
        store.save(&sample_notif("u1", "B")).unwrap();
        store.mark_read(&n1.id).unwrap();
        let deleted = store.delete_read("u1").unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn load_for_user_newest_first() {
        let store = setup();
        store.save(&sample_notif("u1", "First")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        store.save(&sample_notif("u1", "Second")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        store.save(&sample_notif("u1", "Third")).unwrap();
        let loaded = store.load_for_user("u1", 10).unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].title, "Third");
        assert_eq!(loaded[2].title, "First");
    }

    #[test]
    fn load_for_user_respects_limit() {
        let store = setup();
        for i in 0..10 {
            store.save(&sample_notif("u1", &format!("N{}", i))).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let loaded = store.load_for_user("u1", 5).unwrap();
        assert_eq!(loaded.len(), 5);
    }

    #[test]
    fn load_all_works() {
        let store = setup();
        store.save(&sample_notif("u1", "A")).unwrap();
        store.save(&sample_notif("u2", "B")).unwrap();
        let loaded = store.load_all(10).unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[test]
    fn severity_roundtrip() {
        let store = setup();
        let mut n = sample_notif("u1", "A");
        n.severity = NotificationSeverity::Error;
        store.save(&n).unwrap();
        let loaded = store.load_for_user("u1", 1).unwrap();
        assert_eq!(loaded[0].severity, NotificationSeverity::Error);
    }

    #[test]
    fn link_and_icon_roundtrip() {
        let store = setup();
        let n = sample_notif("u1", "A")
            .with_link("/billing")
            .with_icon("💰");
        store.save(&n).unwrap();
        let loaded = store.load_for_user("u1", 1).unwrap();
        assert_eq!(loaded[0].link, Some("/billing".into()));
        assert_eq!(loaded[0].icon, Some("💰".into()));
    }

    #[test]
    fn update_read_status_persists() {
        let store = setup();
        let n = sample_notif("u1", "A");
        store.save(&n).unwrap();
        store.mark_read(&n.id).unwrap();
        let loaded = store.load_for_user("u1", 1).unwrap();
        assert!(loaded[0].read);
    }
}
