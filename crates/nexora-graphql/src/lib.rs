//! # Nexora GraphQL Endpoint
//!
//! An alternative to the REST API for clients that need flexible queries.
//! Built on `async-graphql` + `async-graphql-axum`.
//!
//! ## Schema overview
//!
//! ```graphql
//! type Query {
//!   me: User
//!   user(id: ID!): User
//!   users(limit: Int = 10, offset: Int = 0): [User!]!
//!   package(id: ID!, version: String): Package
//!   packages(limit: Int = 10): [Package!]!
//!   invoice(id: ID!): Invoice
//!   invoices(customerId: ID!, limit: Int = 10): [Invoice!]!
//!   events(fromId: ID = 0, limit: Int = 100): [Event!]!
//!   health: HealthStatus!
//! }
//!
//! type Mutation {
//!   createNotification(input: CreateNotificationInput!): Notification!
//!   markNotificationRead(id: ID!): Boolean!
//!   revokeSession(id: ID!): Boolean!
//! }
//!
//! type Subscription {
//!   events(prefix: String = ""): Event!
//! }
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! # use nexora_graphql::build_schema;
//! # use nexora_core::NexoraCore;
//! # use std::sync::Arc;
//! # #[tokio::main] async fn main() {
//! let core = Arc::new(NexoraCore::new());
//! let schema = build_schema(core);
//! # }
//! ```

pub mod schema;
pub mod types;

pub use schema::{build_schema, NexoraSchema};
