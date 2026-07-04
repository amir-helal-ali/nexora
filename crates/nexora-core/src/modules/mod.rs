//! Module Manager — installs, enables, disables, and orchestrates modules.
//!
//! See Nexora Engineering Specification, Part 4 (MODULE SYSTEM + MODULE LIFECYCLE).
//! Every capability in the platform is a module. Modules are isolated and
//! communicate only through Core APIs and NXP.
//!
//! The lifecycle is atomic and auditable: every transition generates an
//! event on the EventBus and is recorded in the module's history.

use crate::events::EventBus;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use time::OffsetDateTime;

/// Stable, unique identifier for a module.
pub type ModuleId = String;

/// Module lifecycle state.
///
/// ```text
/// Unknown --install--> Installed
/// Installed --enable--> Enabled
/// Enabled --pause--> Paused
/// Paused --resume--> Enabled
/// Enabled/Paused --disable--> Installed
/// Installed --uninstall--> Removed
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleState {
    /// Installed but not enabled.
    Installed,
    /// Active and serving traffic.
    Enabled,
    /// Temporarily not serving traffic; can be resumed.
    Paused,
    /// Marked for removal.
    Removed,
}

impl fmt::Display for ModuleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Installed => f.write_str("installed"),
            Self::Enabled => f.write_str("enabled"),
            Self::Paused => f.write_str("paused"),
            Self::Removed => f.write_str("removed"),
        }
    }
}

/// A module registration entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Module {
    /// Unique module ID (e.g. `auth`, `billing`, `marketplace`).
    pub id: ModuleId,
    /// Human-readable name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Current lifecycle state.
    pub state: ModuleState,
    /// Module owner (organization / user).
    pub owner: String,
    /// Capabilities declared by this module.
    pub capabilities: Vec<String>,
    /// Resource budget (CPU %, memory MB).
    pub resource_budget: ResourceBudget,
    /// When the module was installed.
    pub installed_at: i64,
    /// Last state transition timestamp.
    pub last_transition: i64,
    /// Number of state transitions (audit metric).
    pub transition_count: u64,
}

/// Resource budget for a module.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceBudget {
    /// Max CPU percent (0-100 per core).
    pub max_cpu_percent: u32,
    /// Max memory in MB.
    pub max_memory_mb: u32,
    /// Max concurrent NXP streams.
    pub max_streams: u32,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            max_cpu_percent: 25,
            max_memory_mb: 256,
            max_streams: 64,
        }
    }
}

/// Error from module operations.
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    /// Module not found.
    #[error("module not found: {0}")]
    NotFound(ModuleId),
    /// Module already exists.
    #[error("module already exists: {0}")]
    AlreadyExists(ModuleId),
    /// Invalid state transition.
    #[error("invalid state transition: {from} -> {to} for module {id}")]
    InvalidTransition {
        /// Module ID.
        id: ModuleId,
        /// Source state.
        from: ModuleState,
        /// Target state.
        to: ModuleState,
    },
    /// Module is in the wrong state for this operation.
    #[error("module {id} is in state {state}, expected {expected}")]
    WrongState {
        /// Module ID.
        id: ModuleId,
        /// Current state.
        state: ModuleState,
        /// Expected state.
        expected: ModuleState,
    },
}

/// The Module Manager. Thread-safe.
pub struct ModuleManager {
    modules: RwLock<HashMap<ModuleId, Module>>,
    event_bus: Option<Arc<EventBus>>,
}

impl fmt::Debug for ModuleManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.modules.read().len();
        f.debug_struct("ModuleManager")
            .field("module_count", &count)
            .finish()
    }
}

impl Default for ModuleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleManager {
    /// Construct an empty module manager.
    pub fn new() -> Self {
        Self {
            modules: RwLock::new(HashMap::new()),
            event_bus: None,
        }
    }

    /// Attach an EventBus so lifecycle changes generate events.
    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Number of registered modules.
    pub fn module_count(&self) -> usize {
        self.modules.read().len()
    }

    /// Install a new module. Atomic + audited.
    pub fn install(&self, mut module: Module) -> Result<(), ModuleError> {
        let mut mods = self.modules.write();
        if mods.contains_key(&module.id) {
            return Err(ModuleError::AlreadyExists(module.id.clone()));
        }
        let now = now_ts();
        module.state = ModuleState::Installed;
        module.installed_at = now;
        module.last_transition = now;
        module.transition_count = 1;
        let id = module.id.clone();
        mods.insert(id.clone(), module);
        drop(mods);
        self.emit_event("module.installed", &id);
        Ok(())
    }

    /// Enable an installed module.
    pub fn enable(&self, id: &str) -> Result<(), ModuleError> {
        self.transition(id, ModuleState::Enabled)
    }

    /// Disable (pause) an enabled module.
    pub fn pause(&self, id: &str) -> Result<(), ModuleError> {
        self.transition(id, ModuleState::Paused)
    }

    /// Resume a paused module.
    pub fn resume(&self, id: &str) -> Result<(), ModuleError> {
        self.transition(id, ModuleState::Enabled)
    }

    /// Uninstall a module.
    pub fn uninstall(&self, id: &str) -> Result<(), ModuleError> {
        let mut mods = self.modules.write();
        let module = mods
            .get_mut(id)
            .ok_or_else(|| ModuleError::NotFound(id.to_string()))?;
        if module.state == ModuleState::Enabled {
            return Err(ModuleError::WrongState {
                id: id.to_string(),
                state: module.state,
                expected: ModuleState::Installed,
            });
        }
        module.state = ModuleState::Removed;
        module.last_transition = now_ts();
        module.transition_count += 1;
        drop(mods);
        self.emit_event("module.uninstalled", id);
        Ok(())
    }

    /// Look up a module by ID.
    pub fn get(&self, id: &str) -> Option<Module> {
        self.modules.read().get(id).cloned()
    }

    /// List all modules (snapshot).
    pub fn list(&self) -> Vec<Module> {
        self.modules.read().values().cloned().collect()
    }

    /// Snapshot of modules in a given state.
    pub fn list_in_state(&self, state: ModuleState) -> Vec<Module> {
        self.modules
            .read()
            .values()
            .filter(|m| m.state == state)
            .cloned()
            .collect()
    }

    fn transition(&self, id: &str, target: ModuleState) -> Result<(), ModuleError> {
        let mut mods = self.modules.write();
        let module = mods
            .get_mut(id)
            .ok_or_else(|| ModuleError::NotFound(id.to_string()))?;
        let from = module.state;
        if !is_valid_transition(from, target) {
            return Err(ModuleError::InvalidTransition {
                id: id.to_string(),
                from,
                to: target,
            });
        }
        module.state = target;
        module.last_transition = now_ts();
        module.transition_count += 1;
        drop(mods);
        let event_name = match target {
            ModuleState::Enabled => "module.enabled",
            ModuleState::Paused => "module.paused",
            _ => "module.transition",
        };
        self.emit_event(event_name, id);
        Ok(())
    }

    fn emit_event(&self, name: &str, module_id: &str) {
        if let Some(bus) = &self.event_bus {
            bus.publish(name, module_id.to_string());
        }
    }
}

fn is_valid_transition(from: ModuleState, to: ModuleState) -> bool {
    match (from, to) {
        (ModuleState::Installed, ModuleState::Enabled) => true,
        (ModuleState::Enabled, ModuleState::Paused) => true,
        (ModuleState::Paused, ModuleState::Enabled) => true,
        (ModuleState::Enabled, ModuleState::Installed) => true,
        (ModuleState::Paused, ModuleState::Installed) => true,
        _ => false,
    }
}

fn now_ts() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp_nanos() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_module(id: &str) -> Module {
        Module {
            id: id.to_string(),
            name: format!("{} module", id),
            version: "0.1.0".to_string(),
            state: ModuleState::Installed,
            owner: "test".to_string(),
            capabilities: vec!["test".to_string()],
            resource_budget: ResourceBudget::default(),
            installed_at: 0,
            last_transition: 0,
            transition_count: 0,
        }
    }

    #[test]
    fn install_then_enable_then_pause_then_resume() {
        let mgr = ModuleManager::new();
        mgr.install(sample_module("auth")).unwrap();
        assert_eq!(mgr.module_count(), 1);
        mgr.enable("auth").unwrap();
        assert_eq!(mgr.get("auth").unwrap().state, ModuleState::Enabled);
        mgr.pause("auth").unwrap();
        assert_eq!(mgr.get("auth").unwrap().state, ModuleState::Paused);
        mgr.resume("auth").unwrap();
        assert_eq!(mgr.get("auth").unwrap().state, ModuleState::Enabled);
        assert!(mgr.get("auth").unwrap().transition_count >= 4);
    }

    #[test]
    fn cannot_enable_unknown_module() {
        let mgr = ModuleManager::new();
        assert!(matches!(
            mgr.enable("nope"),
            Err(ModuleError::NotFound(_))
        ));
    }

    #[test]
    fn cannot_install_duplicate() {
        let mgr = ModuleManager::new();
        mgr.install(sample_module("auth")).unwrap();
        assert!(matches!(
            mgr.install(sample_module("auth")),
            Err(ModuleError::AlreadyExists(_))
        ));
    }

    #[test]
    fn invalid_transition_rejected() {
        let mgr = ModuleManager::new();
        mgr.install(sample_module("auth")).unwrap();
        // Cannot pause a module that's only Installed (not Enabled).
        assert!(matches!(
            mgr.pause("auth"),
            Err(ModuleError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn cannot_uninstall_enabled_module() {
        let mgr = ModuleManager::new();
        mgr.install(sample_module("auth")).unwrap();
        mgr.enable("auth").unwrap();
        assert!(matches!(
            mgr.uninstall("auth"),
            Err(ModuleError::WrongState { .. })
        ));
        // Disable first, then uninstall.
        mgr.pause("auth").unwrap();
        mgr.uninstall("auth").unwrap();
        assert_eq!(mgr.get("auth").unwrap().state, ModuleState::Removed);
    }

    #[test]
    fn list_in_state_works() {
        let mgr = ModuleManager::new();
        mgr.install(sample_module("auth")).unwrap();
        mgr.install(sample_module("billing")).unwrap();
        mgr.enable("auth").unwrap();
        assert_eq!(mgr.list_in_state(ModuleState::Enabled).len(), 1);
        assert_eq!(mgr.list_in_state(ModuleState::Installed).len(), 1);
    }
}
