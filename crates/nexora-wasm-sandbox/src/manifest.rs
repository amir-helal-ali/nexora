//! Plugin manifest — describes a plugin to be loaded into the sandbox.

use crate::capabilities::Capability;
use crate::error::{SandboxError, SandboxResult};
use serde::{Deserialize, Serialize};

/// Manifest for a plugin.
///
/// A plugin is uniquely identified by its `id` plus `version`. The sandbox
/// enforces `fuel`, `memory_bytes`, and `timeout_ms` as hard limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin identifier (e.g. `com.example.sendgrid-adapter`).
    pub id: String,
    /// SemVer version string.
    pub version: String,
    /// Raw WASM module bytes.
    pub wasm_bytes: Vec<u8>,
    /// Capabilities granted to this plugin.
    pub capabilities: Vec<Capability>,
    /// Maximum fuel (instruction count) per execution. 1 fuel ≈ 1 wasm op.
    pub fuel: u64,
    /// Maximum linear memory in bytes.
    pub memory_bytes: usize,
    /// Wall-clock timeout in milliseconds.
    pub timeout_ms: u64,
}

/// Output returned from a plugin execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginOutput {
    /// Plugin ran successfully and returned raw bytes.
    Bytes(Vec<u8>),
    /// Plugin ran successfully and returned nothing.
    Unit,
}

impl PluginManifest {
    /// Validate the manifest. Returns an error if limits are unreasonable.
    pub fn validate(&self) -> SandboxResult<()> {
        if self.id.is_empty() {
            return Err(SandboxError::InvalidManifest("id is empty".into()));
        }
        if self.version.is_empty() {
            return Err(SandboxError::InvalidManifest("version is empty".into()));
        }
        if self.wasm_bytes.is_empty() {
            return Err(SandboxError::InvalidManifest("wasm_bytes is empty".into()));
        }
        if self.fuel == 0 {
            return Err(SandboxError::InvalidManifest("fuel must be > 0".into()));
        }
        if self.memory_bytes == 0 || self.memory_bytes > MAX_MEMORY_BYTES {
            return Err(SandboxError::InvalidManifest(format!(
                "memory_bytes must be in 1..={MAX_MEMORY_BYTES}, got {}",
                self.memory_bytes
            )));
        }
        if self.timeout_ms == 0 || self.timeout_ms > MAX_TIMEOUT_MS {
            return Err(SandboxError::InvalidManifest(format!(
                "timeout_ms must be in 1..={MAX_TIMEOUT_MS}, got {}",
                self.timeout_ms
            )));
        }
        Ok(())
    }

    /// Default resource budget for a low-trust plugin.
    pub fn default_budget() -> (u64, usize, u64) {
        (DEFAULT_FUEL, DEFAULT_MEMORY_BYTES, DEFAULT_TIMEOUT_MS)
    }
}

/// Defaults: conservative — these match Part 9's low-resource deployment guidance.
pub const DEFAULT_FUEL: u64 = 10_000_000; // ~10M ops
pub const DEFAULT_MEMORY_BYTES: usize = 32 * 1024 * 1024; // 32 MiB
pub const DEFAULT_TIMEOUT_MS: u64 = 5_000; // 5 seconds

/// Hard caps — a manifest cannot exceed these.
pub const MAX_FUEL: u64 = 1_000_000_000; // 1B ops
pub const MAX_MEMORY_BYTES: usize = 256 * 1024 * 1024; // 256 MiB
pub const MAX_TIMEOUT_MS: u64 = 30_000; // 30 seconds

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> PluginManifest {
        PluginManifest {
            id: "test.plugin".into(),
            version: "1.0.0".into(),
            wasm_bytes: vec![0x00, 0x61, 0x73, 0x6d], // "\0asm" magic
            capabilities: vec![Capability::Log],
            fuel: 1_000,
            memory_bytes: 1024 * 1024,
            timeout_ms: 1_000,
        }
    }

    #[test]
    fn valid_manifest_passes() {
        assert!(valid_manifest().validate().is_ok());
    }

    #[test]
    fn rejects_empty_id() {
        let mut m = valid_manifest();
        m.id.clear();
        assert!(matches!(m.validate(), Err(SandboxError::InvalidManifest(_))));
    }

    #[test]
    fn rejects_empty_version() {
        let mut m = valid_manifest();
        m.version.clear();
        assert!(matches!(m.validate(), Err(SandboxError::InvalidManifest(_))));
    }

    #[test]
    fn rejects_empty_wasm() {
        let mut m = valid_manifest();
        m.wasm_bytes.clear();
        assert!(matches!(m.validate(), Err(SandboxError::InvalidManifest(_))));
    }

    #[test]
    fn rejects_zero_fuel() {
        let mut m = valid_manifest();
        m.fuel = 0;
        assert!(matches!(m.validate(), Err(SandboxError::InvalidManifest(_))));
    }

    #[test]
    fn rejects_oversized_memory() {
        let mut m = valid_manifest();
        m.memory_bytes = MAX_MEMORY_BYTES + 1;
        assert!(matches!(m.validate(), Err(SandboxError::InvalidManifest(_))));
    }

    #[test]
    fn rejects_oversized_timeout() {
        let mut m = valid_manifest();
        m.timeout_ms = MAX_TIMEOUT_MS + 1;
        assert!(matches!(m.validate(), Err(SandboxError::InvalidManifest(_))));
    }

    #[test]
    fn serde_roundtrip() {
        let m = valid_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let m2: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m.id, m2.id);
        assert_eq!(m.fuel, m2.fuel);
    }

    #[test]
    fn default_budget_is_sane() {
        let (fuel, mem, timeout) = PluginManifest::default_budget();
        assert!(fuel > 0);
        assert!(mem > 0);
        assert!(timeout > 0);
        assert!(fuel <= MAX_FUEL);
        assert!(mem <= MAX_MEMORY_BYTES);
        assert!(timeout <= MAX_TIMEOUT_MS);
    }
}
