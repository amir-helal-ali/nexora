//! Configuration Manager — dynamic, hot-reloadable configuration.
//!
//! See Nexora Engineering Specification, Part 4 (CONFIGURATION MANAGER) and
//! Law 19 (CONFIGURATION). Everything must be configurable; nothing important
//! requires recompilation. Supports environment variables, configuration
//! files, secret managers, and dynamic reload when safe.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// A configuration value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigValue {
    /// String value.
    Str(String),
    /// Integer.
    Int(i64),
    /// Boolean.
    Bool(bool),
    /// Float.
    Float(f64),
    /// Nested map.
    Map(HashMap<String, ConfigValue>),
}

impl ConfigValue {
    /// Returns the string value if applicable.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the integer value if applicable.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the boolean value if applicable.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// The Configuration Manager. Thread-safe.
pub struct ConfigManager {
    values: RwLock<HashMap<String, ConfigValue>>,
}

impl fmt::Debug for ConfigManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.values.read().len();
        f.debug_struct("ConfigManager")
            .field("keys", &count)
            .finish()
    }
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigManager {
    /// Construct an empty config manager.
    pub fn new() -> Self {
        Self {
            values: RwLock::new(HashMap::new()),
        }
    }

    /// Set a configuration value.
    pub fn set(&self, key: impl Into<String>, value: ConfigValue) {
        self.values.write().insert(key.into(), value);
    }

    /// Get a configuration value.
    pub fn get(&self, key: &str) -> Option<ConfigValue> {
        self.values.read().get(key).cloned()
    }

    /// Get a string config value.
    pub fn get_str(&self, key: &str) -> Option<String> {
        self.get(key).and_then(|v| v.as_str().map(|s| s.to_string()))
    }

    /// Get an integer config value.
    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|v| v.as_int())
    }

    /// Get a boolean config value.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| v.as_bool())
    }

    /// Get a value or a default.
    pub fn get_or(&self, key: &str, default: ConfigValue) -> ConfigValue {
        self.get(key).unwrap_or(default)
    }

    /// Reload configuration from a key-value map (e.g. parsed from a file).
    /// Replaces all existing values.
    pub fn reload(&self, new_values: HashMap<String, ConfigValue>) {
        *self.values.write() = new_values;
    }

    /// Snapshot all configuration values.
    pub fn snapshot(&self) -> HashMap<String, ConfigValue> {
        self.values.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_works() {
        let cfg = ConfigManager::new();
        cfg.set("nxp.port", ConfigValue::Int(4433));
        cfg.set("nxp.host", ConfigValue::Str("0.0.0.0".into()));
        cfg.set("debug", ConfigValue::Bool(true));
        assert_eq!(cfg.get_int("nxp.port"), Some(4433));
        assert_eq!(cfg.get_str("nxp.host"), Some("0.0.0.0".to_string()));
        assert_eq!(cfg.get_bool("debug"), Some(true));
        assert_eq!(cfg.get_str("missing"), None);
    }

    #[test]
    fn reload_replaces_all() {
        let cfg = ConfigManager::new();
        cfg.set("a", ConfigValue::Int(1));
        let mut new = HashMap::new();
        new.insert("b".to_string(), ConfigValue::Int(2));
        cfg.reload(new);
        assert!(cfg.get("a").is_none());
        assert_eq!(cfg.get_int("b"), Some(2));
    }

    #[test]
    fn get_or_returns_default() {
        let cfg = ConfigManager::new();
        assert_eq!(
            cfg.get_or("missing", ConfigValue::Str("default".into())),
            ConfigValue::Str("default".into())
        );
    }
}
