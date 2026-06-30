//! Plugin System — signed, sandboxed, resource-limited extensions.
//!
//! See Nexora Engineering Specification, Part 4 (PLUGIN SYSTEM) and Part 5
//! (PACKAGE MODEL + SANDBOX ENVIRONMENT). Plugins extend modules without
//! modifying core logic. They run inside isolated sandboxes and must be
//! digitally signed.

use crate::events::EventBus;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Unique plugin ID.
pub type PluginId = String;

/// Plugin lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginState {
    /// Plugin registered but not yet verified.
    Pending,
    /// Verified and ready to install.
    Verified,
    /// Actively running.
    Active,
    /// Stopped but installed.
    Stopped,
    /// Failed verification or runtime error.
    Failed,
    /// Removed from the system.
    Removed,
}

impl fmt::Display for PluginState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => f.write_str("pending"),
            Self::Verified => f.write_str("verified"),
            Self::Active => f.write_str("active"),
            Self::Stopped => f.write_str("stopped"),
            Self::Failed => f.write_str("failed"),
            Self::Removed => f.write_str("removed"),
        }
    }
}

/// Plugin manifest — required metadata for every plugin.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin ID.
    pub id: PluginId,
    /// Human-readable name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Owner (organization / developer).
    pub owner: String,
    /// Ed25519 public key of the signer (32 bytes, hex-encoded).
    pub signer_public_key: String,
    /// Capabilities declared (e.g. `nxp.command.execute`, `event.publish`).
    pub capabilities: Vec<String>,
    /// Resource limits.
    pub resource_limits: PluginResourceLimits,
    /// Module this plugin extends.
    pub extends_module: String,
    /// Ed25519 signature over the manifest (64 bytes, hex-encoded).
    pub signature: String,
}

/// Resource limits for a plugin.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginResourceLimits {
    /// Max CPU percent.
    pub max_cpu_percent: u32,
    /// Max memory MB.
    pub max_memory_mb: u32,
    /// Max NXP commands per second.
    pub max_commands_per_sec: u32,
    /// Max execution time per command (ms).
    pub max_command_duration_ms: u32,
}

impl Default for PluginResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_percent: 10,
            max_memory_mb: 64,
            max_commands_per_sec: 100,
            max_command_duration_ms: 1000,
        }
    }
}

/// A loaded plugin instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plugin {
    /// Manifest.
    pub manifest: PluginManifest,
    /// Current state.
    pub state: PluginState,
    /// Integrity hash (SHA-256 of canonical manifest bytes).
    pub integrity_hash: String,
    /// When the plugin was registered (unix nanos).
    pub registered_at: i64,
    /// Number of commands executed.
    pub commands_executed: u64,
}

/// Error from plugin operations.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// Plugin not found.
    #[error("plugin not found: {0}")]
    NotFound(PluginId),
    /// Plugin already registered.
    #[error("plugin already registered: {0}")]
    AlreadyExists(PluginId),
    /// Signature verification failed.
    #[error("signature verification failed for plugin {0}")]
    SignatureFailed(PluginId),
    /// Plugin is in the wrong state.
    #[error("plugin {id} in state {state}, expected {expected}")]
    WrongState {
        /// Plugin ID.
        id: PluginId,
        /// Current state.
        state: PluginState,
        /// Expected state.
        expected: PluginState,
    },
}

/// Plugin Manager. Thread-safe.
pub struct PluginManager {
    plugins: RwLock<HashMap<PluginId, Plugin>>,
    event_bus: Option<Arc<EventBus>>,
}

impl fmt::Debug for PluginManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.plugins.read().len();
        f.debug_struct("PluginManager")
            .field("plugin_count", &count)
            .finish()
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    /// Construct an empty plugin manager.
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            event_bus: None,
        }
    }

    /// Attach an EventBus.
    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Number of registered plugins.
    pub fn plugin_count(&self) -> usize {
        self.plugins.read().len()
    }

    /// Register a new plugin. The manifest is hashed for integrity. In a
    /// production system, the Ed25519 signature would be cryptographically
    /// verified here; for the v0.1 MVP we compute the integrity hash and
    /// record the signature for later verification.
    pub fn register(&self, manifest: PluginManifest) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write();
        if plugins.contains_key(&manifest.id) {
            return Err(PluginError::AlreadyExists(manifest.id.clone()));
        }
        let integrity_hash = compute_integrity_hash(&manifest);
        let plugin = Plugin {
            manifest,
            state: PluginState::Pending,
            integrity_hash,
            registered_at: 0,
            commands_executed: 0,
        };
        let id = plugin.manifest.id.clone();
        plugins.insert(id.clone(), plugin);
        drop(plugins);
        self.emit_event("plugin.registered", &id);
        Ok(())
    }

    /// Verify a plugin (mark as ready to activate).
    pub fn verify(&self, id: &str) -> Result<(), PluginError> {
        self.transition(id, PluginState::Verified, PluginState::Pending)
    }

    /// Activate a verified plugin.
    pub fn activate(&self, id: &str) -> Result<(), PluginError> {
        self.transition(id, PluginState::Active, PluginState::Verified)
    }

    /// Stop an active plugin.
    pub fn stop(&self, id: &str) -> Result<(), PluginError> {
        self.transition(id, PluginState::Stopped, PluginState::Active)
    }

    /// Remove a plugin.
    pub fn remove(&self, id: &str) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write();
        let plugin = plugins
            .get_mut(id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
        plugin.state = PluginState::Removed;
        drop(plugins);
        self.emit_event("plugin.removed", id);
        Ok(())
    }

    /// Record that a plugin executed a command.
    pub fn record_command(&self, id: &str) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write();
        let plugin = plugins
            .get_mut(id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
        if plugin.state != PluginState::Active {
            return Err(PluginError::WrongState {
                id: id.to_string(),
                state: plugin.state,
                expected: PluginState::Active,
            });
        }
        plugin.commands_executed += 1;
        Ok(())
    }

    /// Get a plugin by ID.
    pub fn get(&self, id: &str) -> Option<Plugin> {
        self.plugins.read().get(id).cloned()
    }

    /// List all plugins (snapshot).
    pub fn list(&self) -> Vec<Plugin> {
        self.plugins.read().values().cloned().collect()
    }

    fn transition(
        &self,
        id: &str,
        target: PluginState,
        expected: PluginState,
    ) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write();
        let plugin = plugins
            .get_mut(id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
        if plugin.state != expected {
            return Err(PluginError::WrongState {
                id: id.to_string(),
                state: plugin.state,
                expected,
            });
        }
        plugin.state = target;
        drop(plugins);
        let event_name = match target {
            PluginState::Verified => "plugin.verified",
            PluginState::Active => "plugin.activated",
            PluginState::Stopped => "plugin.stopped",
            _ => "plugin.transition",
        };
        self.emit_event(event_name, id);
        Ok(())
    }

    fn emit_event(&self, name: &str, plugin_id: &str) {
        if let Some(bus) = &self.event_bus {
            bus.publish(name, plugin_id.to_string());
        }
    }
}

/// Compute the SHA-256 integrity hash of a manifest. The hash is taken over
/// the canonical MessagePack encoding of the manifest with the `signature`
/// field blanked out (so the signature can sign the hash).
fn compute_integrity_hash(manifest: &PluginManifest) -> String {
    let mut canonical = manifest.clone();
    canonical.signature = String::new();
    let bytes = rmp_serde::to_vec_named(&canonical).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest(id: &str) -> PluginManifest {
        PluginManifest {
            id: id.to_string(),
            name: format!("{} plugin", id),
            version: "0.1.0".to_string(),
            owner: "test".to_string(),
            signer_public_key: "00".repeat(32),
            capabilities: vec!["nxp.command.execute".to_string()],
            resource_limits: PluginResourceLimits::default(),
            extends_module: "auth".to_string(),
            signature: "00".repeat(64),
        }
    }

    #[test]
    fn register_verify_activate_stop_remove() {
        let mgr = PluginManager::new();
        mgr.register(sample_manifest("p1")).unwrap();
        assert_eq!(mgr.get("p1").unwrap().state, PluginState::Pending);
        mgr.verify("p1").unwrap();
        assert_eq!(mgr.get("p1").unwrap().state, PluginState::Verified);
        mgr.activate("p1").unwrap();
        assert_eq!(mgr.get("p1").unwrap().state, PluginState::Active);
        mgr.record_command("p1").unwrap();
        assert_eq!(mgr.get("p1").unwrap().commands_executed, 1);
        mgr.stop("p1").unwrap();
        assert_eq!(mgr.get("p1").unwrap().state, PluginState::Stopped);
        mgr.remove("p1").unwrap();
        assert_eq!(mgr.get("p1").unwrap().state, PluginState::Removed);
    }

    #[test]
    fn cannot_activate_unverified() {
        let mgr = PluginManager::new();
        mgr.register(sample_manifest("p1")).unwrap();
        assert!(matches!(
            mgr.activate("p1"),
            Err(PluginError::WrongState { .. })
        ));
    }

    #[test]
    fn cannot_record_command_on_stopped() {
        let mgr = PluginManager::new();
        mgr.register(sample_manifest("p1")).unwrap();
        mgr.verify("p1").unwrap();
        mgr.activate("p1").unwrap();
        mgr.stop("p1").unwrap();
        assert!(matches!(
            mgr.record_command("p1"),
            Err(PluginError::WrongState { .. })
        ));
    }

    #[test]
    fn integrity_hash_is_stable() {
        let m = sample_manifest("p1");
        let h1 = compute_integrity_hash(&m);
        let h2 = compute_integrity_hash(&m);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
    }

    #[test]
    fn integrity_hash_changes_on_content_change() {
        let m1 = sample_manifest("p1");
        let mut m2 = m1.clone();
        m2.name = "different".to_string();
        assert_ne!(compute_integrity_hash(&m1), compute_integrity_hash(&m2));
    }
}
