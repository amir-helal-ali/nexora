//! # Nexora WASM Plugin Sandbox
//!
//! Secure runtime for user-supplied WebAssembly modules (Part 9 of the spec).
//!
//! ## Design
//!
//! - Each plugin runs in its own `wasmtime::Engine` with strict resource limits.
//! - Capabilities are explicit: a plugin can only call host functions it has been granted.
//! - Fuel-based execution prevents infinite loops.
//! - Memory is capped per-instance.
//! - Wall-clock and CPU timeouts are enforced by the scheduler.
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
