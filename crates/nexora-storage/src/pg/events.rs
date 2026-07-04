//! PostgreSQL event store — the durable event-sourcing log.

use crate::pg::{PgError, PgPool};
use nexora_core::events::{Event, EventId, EventPayload};

/// PostgreSQL event store.
pub struct PgEventStore {
    pool: PgPool,
}

impl PgEventStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Append an event to the log. Returns the assigned ID.
    pub async fn append(
        &self,
        name: &str,
        payload: &EventPayload,
        occurred_at: i64,
    ) -> Result<EventId, PgError> {
        let (kind, bytes) = encode_payload(payload);
        let conn = self.pool.get_conn().await?;
        let row = conn.query_one(
            "INSERT INTO events (name, payload_kind, payload_bytes, occurred_at)
             VALUES ($1, $2, $3, $4)
             RETURNING id",
            &[&name, &kind, &bytes, &occurred_at],
        ).await?;
        let id: i64 = row.get(0);
        Ok(id as EventId)
    }

    /// Load events with ID >= `from_id`, in ascending order.
    pub async fn replay(&self, from_id: EventId, limit: i64) -> Result<Vec<Event>, PgError> {
        let conn = self.pool.get_conn().await?;
        let rows = conn.query(
            "SELECT id, name, payload_kind, payload_bytes, occurred_at
             FROM events
             WHERE id >= $1
             ORDER BY id ASC
             LIMIT $2",
            &[&(from_id as i64), &limit],
        ).await?;
        rows.iter().map(Self::row_to_event).collect()
    }

    /// Replay events matching a name prefix.
    pub async fn replay_filtered(
        &self,
        from_id: EventId,
        prefix: &str,
        limit: i64,
    ) -> Result<Vec<Event>, PgError> {
        let pattern = format!("{prefix}%");
        let conn = self.pool.get_conn().await?;
        let rows = conn.query(
            "SELECT id, name, payload_kind, payload_bytes, occurred_at
             FROM events
             WHERE id >= $1 AND name LIKE $2
             ORDER BY id ASC
             LIMIT $3",
            &[&(from_id as i64), &pattern, &limit],
        ).await?;
        rows.iter().map(Self::row_to_event).collect()
    }

    /// Total event count.
    pub async fn count(&self) -> Result<i64, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM events", &[]).await?;
        Ok(row.get(0))
    }

    fn row_to_event(row: &tokio_postgres::Row) -> Result<Event, PgError> {
        let id: i64 = row.get(0);
        let name: String = row.get(1);
        let kind: i16 = row.get(2);
        let bytes: Vec<u8> = row.get(3);
        let occurred_at: i64 = row.get(4);
        let payload = decode_payload(kind as u8, &bytes)?;
        Ok(Event {
            id: id as EventId,
            name,
            payload,
            timestamp: occurred_at,
        })
    }
}

/// Encode an EventPayload as (kind, bytes) for storage.
///
/// - kind 0 = Empty
/// - kind 1 = Text (UTF-8 bytes)
/// - kind 2 = Bytes (raw)
fn encode_payload(p: &EventPayload) -> (i16, Vec<u8>) {
    match p {
        EventPayload::Empty => (0, vec![]),
        EventPayload::Text(s) => (1, s.as_bytes().to_vec()),
        EventPayload::Bytes(b) => (2, b.clone()),
    }
}

/// Decode a payload from storage.
fn decode_payload(kind: u8, bytes: &[u8]) -> Result<EventPayload, PgError> {
    match kind {
        0 => Ok(EventPayload::Empty),
        1 => Ok(EventPayload::Text(
            String::from_utf8(bytes.to_vec()).map_err(|e| PgError::Serde(e.to_string()))?,
        )),
        2 => Ok(EventPayload::Bytes(bytes.to_vec())),
        k => Err(PgError::Serde(format!("unknown payload kind: {k}"))),
    }
}
