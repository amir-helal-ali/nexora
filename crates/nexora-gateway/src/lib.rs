//! Nexora API Gateway — HTTP ↔ NXP translation layer.
//!
//! See Nexora Engineering Specification, Part 6 (BACKEND ARCHITECTURE):
//! "External communication ONLY via API Gateway." This gateway is the only
//! HTTP surface of the platform. Every HTTP request is translated into an
//! NXP command and dispatched to the appropriate service (Auth, Core, etc.).
//!
//! # Architecture
//!
//! ```text
//!  Browser / curl / external HTTP client
//!                  │
//!                  ▼
//!         ┌─────────────────┐
//!         │   API Gateway   │  ← axum HTTP server
//!         │   (this crate)  │
//!         └────────┬────────┘
//!                  │ JSON → MessagePack translation
//!                  │ Bearer token validation
//!                  ▼
//!         ┌─────────────────┐
//!         │  AuthHandler    │  ← in-process (no NXP round-trip)
//!         │  CoreHandler    │
//!         └─────────────────┘
//! ```
//!
//! # Routing
//!
//! - `POST /api/auth/login`     → AUTH_LOGIN
//! - `POST /api/auth/logout`    → AUTH_LOGOUT
//! - `POST /api/auth/refresh`   → AUTH_REFRESH
//! - `POST /api/core/ping`      → PING
//! - `POST /api/core/events`    → PUBLISH_EVENT
//! - `GET  /api/core/events`    → REPLAY_EVENTS
//! - `GET  /api/health`         → gateway liveness
//! - `GET  /api/openapi.json`   → OpenAPI 3.0 spec
//!
//! # Token Middleware
//!
//! All routes EXCEPT `/api/auth/login`, `/api/auth/refresh`, `/api/health`,
//! and `/api/openapi.json` require a `Authorization: Bearer <token>` header.
//! The token is verified against the AuthHandler's TokenVerifier.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod middleware;
pub mod routes;
pub mod server;
pub mod spec;

pub use server::GatewayServer;
