//! Event Bus — durable, replayable, ordered event log.
//!
//! See Nexora Engineering Specification, Part 4 (EVENT BUS) and Part 8
//! (EVENT SOURCING). Every action generates events. Events are immutable,
//! append-only, replayable, and durable. They are the source of truth for
//! state derivation.
//!
//! # Implementation
//!
//! This is an in-process event bus suitable for Tier-1 (Edge) deployments.
//! In Tier-2/Tier-3, this bus is backed by NATS or Kafka; the public API
//! is identical.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;
use tokio::sync::broadcast;

/// Monotonically-increasing event ID.
pub type EventId = u64;

/// A single event in the bus.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Monotonic event ID.
    pub id: EventId,
    /// Event name / namespace (e.g. `module.installed`, `user.created`).
    pub name: String,
    /// Payload (currently opaque; structured payloads come in v0.2).
    pub payload: EventPayload,
    /// When the event was published (unix nanos).
    pub timestamp: i64,
}

/// Payload variant for events.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EventPayload {
    /// Simple string payload (e.g. an ID).
    Text(String),
    /// Binary payload.
    Bytes(Vec<u8>),
    /// Empty payload.
    Empty,
}

impl Default for EventPayload {
    fn default() -> Self {
        Self::Empty
    }
}

/// Subscriber handle. Drop to stop receiving events.
pub struct EventSubscriber {
    /// The broadcast receiver.
    pub rx: broadcast::Receiver<Event>,
    /// The filter (event name prefix). Empty = all events.
    pub filter: String,
}

impl fmt::Debug for EventSubscriber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventSubscriber")
            .field("filter", &self.filter)
            .finish()
    }
}

/// The EventBus. Thread-safe.
pub struct EventBus {
    log: RwLock<Vec<Event>>,
    next_id: RwLock<EventId>,
    subscribers: RwLock<Vec<(String, broadcast::Sender<Event>)>>,
    /// Broadcast channel capacity.
    capacity: usize,
    /// Total events ever published (does not decrement on replay).
    published: RwLock<u64>,
}

impl fmt::Debug for EventBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let log_len = self.log.read().len();
        let sub_count = self.subscribers.read().len();
        let published = *self.published.read();
        f.debug_struct("EventBus")
            .field("events_in_log", &log_len)
            .field("subscribers", &sub_count)
            .field("total_published", &published)
            .finish()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// Construct a fresh event bus with default broadcast capacity (1024).
    pub fn new() -> Self {
        Self::with_capacity(1024)
    }

    /// Construct with a custom broadcast capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            log: RwLock::new(Vec::new()),
            next_id: RwLock::new(1),
            subscribers: RwLock::new(Vec::new()),
            capacity,
            published: RwLock::new(0),
        }
    }

    /// Total events published since the bus was created.
    pub fn published_count(&self) -> u64 {
        *self.published.read()
    }

    /// Number of events currently in the log.
    pub fn log_len(&self) -> usize {
        self.log.read().len()
    }

    /// Publish an event with a text payload. Returns the event ID.
    pub fn publish(&self, name: impl Into<String>, payload: impl Into<String>) -> EventId {
        self.publish_event(name.into(), EventPayload::Text(payload.into()))
    }

    /// Publish an event with the given payload. Returns the event.
    pub fn publish_event(&self, name: String, payload: EventPayload) -> EventId {
        let mut next_id = self.next_id.write();
        let id = *next_id;
        *next_id += 1;
        drop(next_id);

        let event = Event {
            id,
            name: name.clone(),
            payload,
            timestamp: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
        };

        self.log.write().push(event.clone());
        *self.published.write() += 1;

        // Broadcast to matching subscribers. Best-effort: if a subscriber's
        // channel is full, we skip it (lagging subscribers catch up via
        // `replay`).
        let subs = self.subscribers.read();
        for (filter, tx) in subs.iter() {
            if filter.is_empty() || event.name.starts_with(filter.as_str()) {
                let _ = tx.send(event.clone());
            }
        }
        id
    }

    /// Subscribe to events matching the given name prefix. Empty prefix
    /// subscribes to all events.
    pub fn subscribe(&self, filter: impl Into<String>) -> EventSubscriber {
        let (tx, rx) = broadcast::channel(self.capacity);
        self.subscribers.write().push((filter.into(), tx));
        EventSubscriber {
            rx,
            filter: String::new(),
        }
    }

    /// Replay events from the given ID (inclusive). Returns a snapshot.
    pub fn replay(&self, from_id: EventId) -> Vec<Event> {
        self.log
            .read()
            .iter()
            .filter(|e| e.id >= from_id)
            .cloned()
            .collect()
    }

    /// Replay events matching the given name prefix, starting from `from_id`.
    pub fn replay_filtered(&self, from_id: EventId, prefix: &str) -> Vec<Event> {
        self.log
            .read()
            .iter()
            .filter(|e| e.id >= from_id && e.name.starts_with(prefix))
            .cloned()
            .collect()
    }

    /// Get a specific event by ID.
    pub fn get(&self, id: EventId) -> Option<Event> {
        self.log.read().iter().find(|e| e.id == id).cloned()
    }

    /// Take a snapshot of the entire log.
    pub fn snapshot(&self) -> Vec<Event> {
        self.log.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_assigns_monotonic_ids() {
        let bus = EventBus::new();
        let id1 = bus.publish("test.event", "a");
        let id2 = bus.publish("test.event", "b");
        let id3 = bus.publish("test.event", "c");
        assert!(id1 < id2);
        assert!(id2 < id3);
        assert_eq!(bus.log_len(), 3);
        assert_eq!(bus.published_count(), 3);
    }

    #[test]
    fn replay_returns_events_from_offset() {
        let bus = EventBus::new();
        let id1 = bus.publish("a.b", "1");
        let id2 = bus.publish("a.b", "2");
        let _id3 = bus.publish("c.d", "3");
        let replayed = bus.replay(id2);
        assert_eq!(replayed.len(), 2);
        assert_eq!(replayed[0].id, id2);
    }

    #[test]
    fn replay_filtered_works() {
        let bus = EventBus::new();
        let _ = bus.publish("user.created", "u1");
        let _ = bus.publish("user.deleted", "u1");
        let _ = bus.publish("user.created", "u2");
        let created = bus.replay_filtered(0, "user.created");
        assert_eq!(created.len(), 2);
    }

    #[tokio::test]
    async fn subscriber_receives_matching_events() {
        let bus = EventBus::new();
        let mut sub = bus.subscribe("user.");
        let _ = bus.publish("user.created", "u1");
        let _ = bus.publish("module.installed", "auth"); // should NOT be received
        let _ = bus.publish("user.deleted", "u1");

        let evt1 = sub.rx.recv().await.unwrap();
        assert_eq!(evt1.name, "user.created");
        let evt2 = sub.rx.recv().await.unwrap();
        assert_eq!(evt2.name, "user.deleted");
        // No third event — the module.installed was filtered out.
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(50),
            sub.rx.recv()
        )
        .await
        .is_err());
    }

    #[test]
    fn events_are_immutable_in_log() {
        let bus = EventBus::new();
        let id = bus.publish("test", "hello");
        let snap1 = bus.snapshot();
        let snap2 = bus.snapshot();
        assert_eq!(snap1, snap2);
        assert_eq!(snap1[0].id, id);
        assert_eq!(snap1[0].payload, EventPayload::Text("hello".into()));
    }
}
