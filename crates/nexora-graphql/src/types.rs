//! GraphQL types — mirror the domain types from nexora-* crates.

use async_graphql::SimpleObject;
use time::OffsetDateTime;

/// A user.
#[derive(SimpleObject, Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub roles: Vec<String>,
    pub created_at: i64,
    pub active: bool,
}

impl From<&nexora_auth::users::User> for User {
    fn from(u: &nexora_auth::users::User) -> Self {
        Self {
            id: u.id.clone(),
            username: u.username.clone(),
            email: u.email.clone(),
            roles: u.roles.clone(),
            created_at: u.created_at,
            active: u.active,
        }
    }
}

/// A package from the marketplace.
#[derive(SimpleObject, Debug, Clone)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub owner_name: String,
    pub install_count: i64,
    pub active_install_count: i64,
    pub installed: bool,
    pub integrity_hash: String,
    pub published_at: i64,
}

impl From<&nexora_marketplace::package::Package> for Package {
    fn from(p: &nexora_marketplace::package::Package) -> Self {
        let v = &p.manifest.version;
        Self {
            id: p.manifest.id.clone(),
            name: p.manifest.name.clone(),
            version: format!("{}.{}.{}", v.major, v.minor, v.patch),
            description: p.manifest.description.clone(),
            owner_name: p.manifest.owner_name.clone(),
            install_count: p.install_count as i64,
            active_install_count: p.active_install_count as i64,
            installed: p.installed,
            integrity_hash: p.integrity_hash.clone(),
            published_at: p.published_at,
        }
    }
}

/// An invoice.
#[derive(SimpleObject, Debug, Clone)]
pub struct Invoice {
    pub id: String,
    pub customer_id: String,
    pub customer_name: String,
    pub total_minor: i64,
    pub currency: String,
    pub status: String,
    pub created_at: i64,
    pub due_at: i64,
    pub paid_at: Option<i64>,
}

impl From<&nexora_billing::types::Invoice> for Invoice {
    fn from(i: &nexora_billing::types::Invoice) -> Self {
        Self {
            id: i.id.clone(),
            customer_id: i.customer_id.clone(),
            customer_name: i.customer_name.clone(),
            total_minor: i.total_minor as i64,
            currency: i.currency.clone(),
            status: i.status.to_string(),
            created_at: i.created_at,
            due_at: i.due_at,
            paid_at: i.paid_at,
        }
    }
}

/// An event from the EventBus.
#[derive(SimpleObject, Debug, Clone)]
pub struct Event {
    pub id: i64,
    pub name: String,
    pub payload: String,
    pub timestamp: i64,
}

impl From<&nexora_core::events::Event> for Event {
    fn from(e: &nexora_core::events::Event) -> Self {
        let payload = match &e.payload {
            nexora_core::events::EventPayload::Text(s) => s.clone(),
            nexora_core::events::EventPayload::Bytes(b) => format!("<{} bytes>", b.len()),
            nexora_core::events::EventPayload::Empty => String::new(),
        };
        Self {
            id: e.id as i64,
            name: e.name.clone(),
            payload,
            timestamp: e.timestamp,
        }
    }
}

/// Platform health status.
#[derive(SimpleObject, Debug, Clone)]
pub struct HealthStatus {
    pub healthy: bool,
    pub modules_total: usize,
    pub modules_active: usize,
    pub events_published: i64,
    pub uptime_seconds: i64,
}

/// Input for creating a notification.
#[derive(async_graphql::InputObject, Debug, Clone)]
pub struct CreateNotificationInput {
    pub user_id: String,
    pub title: String,
    pub body: String,
    pub action_url: Option<String>,
}

/// A notification in the in-app store.
#[derive(SimpleObject, Debug, Clone)]
pub struct InAppNotification {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub body: String,
    pub action_url: Option<String>,
    pub created_at: i64,
    pub read_at: Option<i64>,
}

impl From<&nexora_notifications::InAppNotification> for InAppNotification {
    fn from(n: &nexora_notifications::InAppNotification) -> Self {
        Self {
            id: n.id.clone(),
            user_id: n.user_id.clone(),
            title: n.title.clone(),
            body: n.body.clone(),
            action_url: n.action_url.clone(),
            created_at: n.created_at,
            read_at: n.read_at,
        }
    }
}

fn _now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp_nanos() as i64
}
