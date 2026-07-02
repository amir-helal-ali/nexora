//! بوابة Nexora API — طبقة ترجمة HTTP ↔ NXP.
//!
//! انظر مواصفة Nexora الهندسية، الجزء 6 (بنية الخلفية):
//! "التواصل الخارجي فقط عبر بوابة API." هذه البوابة هي السطح HTTP الوحيد
//! للمنصة. كل طلب HTTP يُترجم إلى أمر NXP ويُرسل إلى الخدمة المناسبة
//! (المصادقة، النواة، إلخ).
//!
//! # البنية المعمارية
//!
//! ```text
//!  متصفح / curl / عميل HTTP خارجي
//!                  │
//!                  ▼
//!         ┌─────────────────┐
//!         │   بوابة API     │  ← خادم axum HTTP
//!         │   (هذه الـ crate)│
//!         └────────┬────────┘
//!                  │ ترجمة JSON → MessagePack
//!                  │ التحقق من رمز Bearer
//!                  ▼
//!         ┌─────────────────┐
//!         │  AuthHandler    │  ← في العملية (لا ذهاب-إياب NXP)
//!         │  CoreHandler    │
//!         └─────────────────┘
//! ```
//!
//! # التوجيه
//!
//! - `POST /api/auth/login`     → AUTH_LOGIN
//! - `POST /api/auth/logout`    → AUTH_LOGOUT
//! - `POST /api/auth/refresh`   → AUTH_REFRESH
//! - `POST /api/core/ping`      → PING
//! - `POST /api/core/events`    → PUBLISH_EVENT
//! - `GET  /api/core/events`    → REPLAY_EVENTS
//! - `GET  /api/health`         → فحص حياة البوابة
//! - `GET  /api/openapi.json`   → مواصفات OpenAPI 3.0
//!
//! # برمجيات الرمز الوسيطة
//!
//! كل المسارات عدا `/api/auth/login`، `/api/auth/refresh`، `/api/health`،
//! و `/api/openapi.json` تتطلب ترويسة `Authorization: Bearer <token>`.
//! يُتحقَّق من الرمز مقابل مدقق الرموز في AuthHandler.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod middleware;
pub mod routes;
pub mod server;
pub mod spec;
pub mod sso;

#[cfg(test)]
pub mod integration_tests;

pub use server::GatewayServer;
