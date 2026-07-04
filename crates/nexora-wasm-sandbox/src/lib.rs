//! # صندوق حماية مكونات Nexora WASM
//!
//! بيئة تشغيل آمنة لوحدات WebAssembly التي يقدمها المستخدم (الجزء 9 من المواصفة).
//!
//! ## التصميم
//!
//! - كل مكون يعمل في `wasmtime::Engine` خاصة به مع حدود موارد صارمة.
//! - القدرات صريحة: المكون يمكنه فقط استدعاء دوال المضيف التي مُنحت له.
//! - التنفيذ القائم على الوقود يمنع الحلقات اللانهائية.
//! - الذاكرة محدودة لكل نسخة.
//! - مهلات زمن الساعة و CPU تُفرض بواسطة المجدول.
//!
//! ## Example
//!
//! ```no_run
//! use nexora_wasm_sandbox::{Sandbox, PluginManifest, Capability};
//!
//! # fn main() -> anyhow::Result<()> {
//! let manifest = PluginManifest {
//!     id: "demo".into(),
//!     version: "1.0.0".into(),
//!     wasm_bytes: vec![],
//!     capabilities: vec![Capability::Log, Capability::ReadConfig],
//!     fuel: 1_000_000,
//!     memory_bytes: 32 * 1024 * 1024,
//!     timeout_ms: 5_000,
//! };
//! let sandbox = Sandbox::new()?;
//! let result = sandbox.execute(&manifest, "run", &[])?;
//! # Ok(())
//! # }
//! ```

pub mod capabilities;
pub mod error;
pub mod manifest;
pub mod sandbox;

pub use capabilities::{Capability, CapabilitySet};
pub use error::{SandboxError, SandboxResult};
pub use manifest::{PluginManifest, PluginOutput};
pub use sandbox::Sandbox;
