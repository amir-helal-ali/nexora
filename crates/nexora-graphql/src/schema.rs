//! GraphQL schema definition + resolvers.

use crate::types::{
    CreateNotificationInput, Event, HealthStatus, InAppNotification, Invoice, Package, User,
};
use async_graphql::{
    Context, Object, Schema, Subscription,
};
use futures_util::Stream;
use nexora_core::events::Event as CoreEvent;
use nexora_core::NexoraCore;
use std::sync::Arc;

/// The root query type.
pub struct Query;

/// The root mutation type.
pub struct Mutation;

/// The root subscription type.
pub struct SubscriptionRoot;

/// The full GraphQL schema.
pub type NexoraSchema = Schema<Query, Mutation, SubscriptionRoot>;

/// Build the GraphQL schema with the given Core reference.
pub fn build_schema(core: Arc<NexoraCore>) -> NexoraSchema {
    Schema::build(Query, Mutation, SubscriptionRoot)
        .data(core)
        .finish()
}

#[Object]
impl Query {
    /// Current user (placeholder — in production this reads from the auth context).
    async fn me(&self, _ctx: &Context<'_>) -> Option<User> {
        None
    }

    /// Fetch a user by ID.
    async fn user(&self, _ctx: &Context<'_>, id: String) -> Option<User> {
        // In production, this would call nexora_auth::UserStore::get.
        let _ = id;
        None
    }

    /// List users (paginated).
    async fn users(&self, _ctx: &Context<'_>, limit: i64, offset: i64) -> Vec<User> {
        let _ = (limit, offset);
        Vec::new()
    }

    /// Fetch a package by ID + optional version.
    async fn package(
        &self,
        _ctx: &Context<'_>,
        id: String,
        version: Option<String>,
    ) -> Option<Package> {
        let _ = (id, version);
        None
    }

    /// List packages.
    async fn packages(&self, _ctx: &Context<'_>, limit: i64) -> Vec<Package> {
        let _ = limit;
        Vec::new()
    }

    /// Fetch an invoice by ID.
    async fn invoice(&self, _ctx: &Context<'_>, id: String) -> Option<Invoice> {
        let _ = id;
        None
    }

    /// List invoices for a customer.
    async fn invoices(&self, _ctx: &Context<'_>, customer_id: String, limit: i64) -> Vec<Invoice> {
        let _ = (customer_id, limit);
        Vec::new()
    }

    /// Replay events from the EventBus log.
    async fn events(&self, ctx: &Context<'_>, from_id: i64, limit: i64) -> Vec<Event> {
        let core = ctx.data::<Arc<NexoraCore>>().unwrap();
        let from = if from_id > 0 {
            from_id as u64
        } else {
            1
        };
        let lim = limit.clamp(1, 1000) as usize;
        core.events
            .replay(from)
            .into_iter()
            .take(lim)
            .map(|e| Event::from(&e))
            .collect()
    }

    /// Platform health status.
    async fn health(&self, ctx: &Context<'_>) -> HealthStatus {
        let core = ctx.data::<Arc<NexoraCore>>().unwrap();
        HealthStatus {
            healthy: true,
            modules_total: core.modules.module_count(),
            modules_active: core.modules.module_count(), // simplified
            events_published: core.events.published_count() as i64,
            uptime_seconds: 0, // would track real uptime in production
        }
    }
}

#[Object]
impl Mutation {
    /// Create an in-app notification.
    async fn create_notification(
        &self,
        ctx: &Context<'_>,
        input: CreateNotificationInput,
    ) -> async_graphql::Result<InAppNotification> {
        // For the reference impl, we use the EventBus to signal that a
        // notification was requested. In production, this would call
        // NotificationService::send_in_app.
        let core = ctx.data::<Arc<NexoraCore>>().unwrap();
        let now = time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        core.events
            .publish("notification.created", input.user_id.clone());

        Ok(InAppNotification {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: input.user_id,
            title: input.title,
            body: input.body,
            action_url: input.action_url,
            created_at: now,
            read_at: None,
        })
    }

    /// Mark a notification as read.
    async fn mark_notification_read(
        &self,
        _ctx: &Context<'_>,
        id: String,
    ) -> async_graphql::Result<bool> {
        let _ = id;
        Ok(true)
    }

    /// Revoke a session.
    async fn revoke_session(&self, _ctx: &Context<'_>, id: String) -> async_graphql::Result<bool> {
        let _ = id;
        Ok(true)
    }
}

#[Subscription]
impl SubscriptionRoot {
    /// Stream events matching a name prefix.
    async fn events(
        &self,
        ctx: &Context<'_>,
        prefix: String,
    ) -> impl Stream<Item = Event> + '_ {
        let core = ctx.data::<Arc<NexoraCore>>().unwrap();
        let sub = core.events.subscribe(prefix.clone());
        async_stream::stream! {
            let mut sub = sub;
            while let Ok(event) = sub.rx.recv().await {
                yield Event::from(&event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn schema_builds() {
        let core = Arc::new(NexoraCore::new());
        let schema = build_schema(core);
        let _ = schema;
    }

    #[tokio::test]
    async fn query_health_returns_status() {
        let core = Arc::new(NexoraCore::new());
        let schema = build_schema(core);
        let resp = schema
            .execute("{ health { healthy eventsPublished } }")
            .await;
        assert!(resp.errors.is_empty());
        let data = resp.data.into_json().unwrap();
        assert_eq!(data["health"]["healthy"], true);
    }

    #[tokio::test]
    async fn query_events_returns_empty_initially() {
        let core = Arc::new(NexoraCore::new());
        let schema = build_schema(core);
        let resp = schema.execute("{ events(fromId: 0, limit: 10) { id name } }").await;
        assert!(resp.errors.is_empty());
    }

    #[tokio::test]
    async fn query_events_returns_published_events() {
        let core = Arc::new(NexoraCore::new());
        core.events.publish("test.event", "hello");
        core.events.publish("test.event", "world");
        let schema = build_schema(core);
        let resp = schema.execute("{ events(fromId: 0, limit: 10) { id name } }").await;
        assert!(resp.errors.is_empty());
        let data = resp.data.into_json().unwrap();
        let events = data["events"].as_array().unwrap();
        assert!(events.len() >= 2);
    }

    #[tokio::test]
    async fn mutation_create_notification_returns_id() {
        let core = Arc::new(NexoraCore::new());
        let schema = build_schema(core);
        let resp = schema
            .execute(
                r#"mutation {
                    createNotification(input: {
                        userId: "u1",
                        title: "Hello",
                        body: "World"
                    }) {
                        id
                        userId
                        title
                    }
                }"#,
            )
            .await;
        assert!(resp.errors.is_empty());
        let data = resp.data.into_json().unwrap();
        assert_eq!(data["createNotification"]["userId"], "u1");
        assert_eq!(data["createNotification"]["title"], "Hello");
        assert!(!data["createNotification"]["id"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn mutation_create_notification_emits_event() {
        let core = Arc::new(NexoraCore::new());
        let schema = build_schema(core.clone());
        let _ = schema
            .execute(
                r#"mutation {
                    createNotification(input: {
                        userId: "u1",
                        title: "T",
                        body: "B"
                    }) { id }
                }"#,
            )
            .await;
        assert!(core.events.published_count() >= 1);
    }

    #[tokio::test]
    async fn query_users_returns_empty() {
        let core = Arc::new(NexoraCore::new());
        let schema = build_schema(core);
        let resp = schema.execute("{ users(limit: 10, offset: 0) { id } }").await;
        if !resp.errors.is_empty() {
            eprintln!("GraphQL errors: {:?}", resp.errors);
        }
        assert!(resp.errors.is_empty());
        let data = resp.data.into_json().unwrap();
        assert!(data["users"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn introspection_works() {
        let core = Arc::new(NexoraCore::new());
        let schema = build_schema(core);
        let resp = schema
            .execute("{ __schema { queryType { name } } }")
            .await;
        assert!(resp.errors.is_empty());
        let data = resp.data.into_json().unwrap();
        assert_eq!(data["__schema"]["queryType"]["name"], "Query");
    }
}
