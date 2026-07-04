//! Plugin capabilities (Part 9 — zero-trust capability model).
//!
//! A plugin must declare which capabilities it needs in its manifest, and the
//! sandbox enforces this at call time. A plugin calling a capability it was
//! not granted is rejected with `SandboxError::UnauthorizedCapability`.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

/// Capabilities a plugin may request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Emit log lines via the host logger.
    Log,
    /// Read configuration values from the host config store.
    ReadConfig,
    /// Publish events to the Nexora EventBus.
    PublishEvent,
    /// Make outbound HTTP calls (network access).
    HttpRequest,
    /// Read from the storage layer (read-only).
    StorageRead,
    /// Write to the storage layer (mutating).
    StorageWrite,
    /// Access the current time (wall clock).
    Clock,
    /// Generate cryptographically secure random bytes.
    Random,
}

impl Capability {
    /// All capabilities, in canonical order. Used for parsing.
    pub const ALL: &'static [Capability] = &[
        Capability::Log,
        Capability::ReadConfig,
        Capability::PublishEvent,
        Capability::HttpRequest,
        Capability::StorageRead,
        Capability::StorageWrite,
        Capability::Clock,
        Capability::Random,
    ];

    /// Stable string identifier (used in manifests).
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Log => "log",
            Capability::ReadConfig => "read_config",
            Capability::PublishEvent => "publish_event",
            Capability::HttpRequest => "http_request",
            Capability::StorageRead => "storage_read",
            Capability::StorageWrite => "storage_write",
            Capability::Clock => "clock",
            Capability::Random => "random",
        }
    }

    /// Parse from a stable string identifier.
    pub fn from_str(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.as_str() == s)
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A granted set of capabilities. Acts as an allow-list at call time.
#[derive(Debug, Clone, Default)]
pub struct CapabilitySet {
    inner: HashSet<Capability>,
}

impl CapabilitySet {
    /// Empty capability set (no permissions).
    pub fn none() -> Self {
        Self::default()
    }

    /// Full capability set (all permissions — for trusted built-in plugins only).
    pub fn all() -> Self {
        Self {
            inner: Capability::ALL.iter().copied().collect(),
        }
    }

    /// Construct from an iterator of capabilities.
    pub fn from_iter<I: IntoIterator<Item = Capability>>(iter: I) -> Self {
        Self {
            inner: iter.into_iter().collect(),
        }
    }

    /// Returns true if the capability is granted.
    pub fn contains(&self, cap: Capability) -> bool {
        self.inner.contains(&cap)
    }

    /// Grant an additional capability.
    pub fn grant(&mut self, cap: Capability) {
        self.inner.insert(cap);
    }

    /// Revoke a capability.
    pub fn revoke(&mut self, cap: Capability) {
        self.inner.remove(&cap);
    }

    /// Number of granted capabilities.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True if no capabilities are granted.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate over granted capabilities.
    pub fn iter(&self) -> impl Iterator<Item = Capability> + '_ {
        self.inner.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_string() {
        for cap in Capability::ALL {
            let s = cap.as_str();
            assert_eq!(Capability::from_str(s), Some(*cap));
        }
    }

    #[test]
    fn unknown_string_returns_none() {
        assert!(Capability::from_str("delete_everything").is_none());
    }

    #[test]
    fn empty_set_grants_nothing() {
        let s = CapabilitySet::none();
        for cap in Capability::ALL {
            assert!(!s.contains(*cap));
        }
        assert!(s.is_empty());
    }

    #[test]
    fn full_set_grants_everything() {
        let s = CapabilitySet::all();
        for cap in Capability::ALL {
            assert!(s.contains(*cap));
        }
        assert_eq!(s.len(), Capability::ALL.len());
    }

    #[test]
    fn grant_and_revoke() {
        let mut s = CapabilitySet::none();
        s.grant(Capability::Log);
        s.grant(Capability::Clock);
        assert!(s.contains(Capability::Log));
        assert!(s.contains(Capability::Clock));
        assert!(!s.contains(Capability::Random));
        assert_eq!(s.len(), 2);

        s.revoke(Capability::Log);
        assert!(!s.contains(Capability::Log));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn from_iter_dedupes() {
        let s = CapabilitySet::from_iter([
            Capability::Log,
            Capability::Log,
            Capability::Clock,
        ]);
        assert_eq!(s.len(), 2);
    }
}
