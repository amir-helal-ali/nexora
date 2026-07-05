//! Package store — in-memory registry of published packages.
//!
//! See Nexora Engineering Specification, Part 5 (PACKAGE MODEL + RATING &
//! TRUST SYSTEM). Each package has trust scores, install counts, and a
//! visibility level.

use crate::package::{Package, PackageId, PackageManifest, Visibility};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Trust scores for a package (0-100 each).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TrustScore {
    /// Security score (signature valid, no known vulns, sandbox-tested).
    pub security: u32,
    /// Performance score (latency, resource usage).
    pub performance: u32,
    /// Stability score (uptime, error rate).
    pub stability: u32,
    /// Community rating (1-5 stars, averaged).
    pub community_rating: f32,
    /// Enterprise rating (1-5 stars, averaged).
    pub enterprise_rating: f32,
}

impl TrustScore {
    /// Construct a fresh trust score for a newly-published package.
    pub fn new_signed() -> Self {
        // A freshly-signed package starts with reasonable defaults.
        Self {
            security: 80, // signed = good baseline
            performance: 70,
            stability: 70,
            community_rating: 0.0,
            enterprise_rating: 0.0,
        }
    }

    /// Aggregate trust score (0-100). Weighted average.
    pub fn aggregate(&self) -> u32 {
        let community = (self.community_rating / 5.0 * 100.0) as u32;
        let enterprise = (self.enterprise_rating / 5.0 * 100.0) as u32;
        let weights_sum = 30 + 20 + 20 + 15 + 15;
        let total = self.security * 30
            + self.performance * 20
            + self.stability * 20
            + community * 15
            + enterprise * 15;
        total / weights_sum
    }

    /// Returns `true` if this package is "low trust" (security < 50).
    /// Low-trust packages are automatically sandbox-restricted per RFC §9.
    pub fn is_low_trust(&self) -> bool {
        self.security < 50
    }
}

impl fmt::Display for TrustScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Trust(sec={}, perf={}, stab={}, community={:.1}★, enterprise={:.1}★, agg={})",
            self.security,
            self.performance,
            self.stability,
            self.community_rating,
            self.enterprise_rating,
            self.aggregate()
        )
    }
}

/// Error from store operations.
#[derive(Debug, thiserror::Error)]
pub enum PackageStoreError {
    /// Package already exists.
    #[error("package already exists: {0}")]
    AlreadyExists(PackageId),
    /// Package not found.
    #[error("package not found: {0}")]
    NotFound(PackageId),
    /// Package version not found.
    #[error("package {0} version {1} not found")]
    VersionNotFound(PackageId, String),
}

/// The package store. Thread-safe.
pub struct PackageStore {
    /// Map: package ID → list of published versions (newest first).
    packages: RwLock<HashMap<PackageId, Vec<Package>>>,
    /// Map: package ID → installed version (for tracking active installs).
    installed: RwLock<HashMap<PackageId, String>>,
    event_bus: Option<Arc<nexora_core::EventBus>>,
}

impl fmt::Debug for PackageStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.packages.read().len();
        let installed = self.installed.read().len();
        f.debug_struct("PackageStore")
            .field("package_ids", &count)
            .field("installed", &installed)
            .finish()
    }
}

impl Default for PackageStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PackageStore {
    /// Construct an empty store.
    pub fn new() -> Self {
        Self {
            packages: RwLock::new(HashMap::new()),
            installed: RwLock::new(HashMap::new()),
            event_bus: None,
        }
    }

    /// Attach an EventBus so lifecycle changes publish events.
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Number of distinct package IDs.
    pub fn package_count(&self) -> usize {
        self.packages.read().len()
    }

    /// Number of installed packages.
    pub fn installed_count(&self) -> usize {
        self.installed.read().len()
    }

    /// Publish a new package. The manifest must already be signed.
    /// Returns the created Package.
    pub fn publish(&self, manifest: PackageManifest) -> Result<Package, PackageStoreError> {
        let id = manifest.id.clone();
        let pkg = Package::from_manifest(manifest);
        {
            let mut packages = self.packages.write();
            let versions = packages.entry(id.clone()).or_default();
            // Check for duplicate version.
            if versions.iter().any(|p| p.manifest.version == pkg.manifest.version) {
                return Err(PackageStoreError::AlreadyExists(format!(
                    "{}@{}",
                    id, pkg.manifest.version
                )));
            }
            versions.push(pkg.clone());
            // Sort newest first.
            versions.sort_by(|a, b| b.manifest.version.cmp(&a.manifest.version));
        }
        if let Some(bus) = &self.event_bus {
            bus.publish(
                "package.published",
                format!("{}@{}", id, pkg.manifest.version),
            );
        }
        Ok(pkg)
    }

    /// Get the latest version of a package.
    pub fn get_latest(&self, id: &str) -> Option<Package> {
        self.packages.read().get(id).and_then(|v| v.first().cloned())
    }

    /// Get a specific version of a package.
    pub fn get_version(&self, id: &str, version: &crate::version::Version) -> Option<Package> {
        self.packages
            .read()
            .get(id)
            .and_then(|v| v.iter().find(|p| &p.manifest.version == version).cloned())
    }

    /// Get all versions of a package (newest first).
    pub fn get_all_versions(&self, id: &str) -> Vec<Package> {
        self.packages
            .read()
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    /// Mark a package version as installed.
    pub fn mark_installed(&self, id: &str, version: &crate::version::Version) -> Result<(), PackageStoreError> {
        let mut packages = self.packages.write();
        let versions = packages
            .get_mut(id)
            .ok_or_else(|| PackageStoreError::NotFound(id.to_string()))?;
        let pkg = versions
            .iter_mut()
            .find(|p| &p.manifest.version == version)
            .ok_or_else(|| PackageStoreError::VersionNotFound(id.to_string(), version.to_string()))?;
        if !pkg.installed {
            pkg.installed = true;
            pkg.install_count += 1;
            pkg.active_install_count += 1;
        }
        drop(packages);
        self.installed.write().insert(id.to_string(), version.to_string());
        if let Some(bus) = &self.event_bus {
            bus.publish("package.installed", format!("{}@{}", id, version));
        }
        Ok(())
    }

    /// Mark a package as uninstalled.
    pub fn mark_uninstalled(&self, id: &str) -> Result<(), PackageStoreError> {
        let mut packages = self.packages.write();
        let installed_version = self
            .installed
            .read()
            .get(id)
            .cloned()
            .ok_or_else(|| PackageStoreError::NotFound(id.to_string()))?;
        if let Some(versions) = packages.get_mut(id) {
            if let Some(pkg) = versions.iter_mut().find(|p| p.manifest.version.to_string() == installed_version) {
                pkg.installed = false;
                pkg.active_install_count = pkg.active_install_count.saturating_sub(1);
            }
        }
        drop(packages);
        self.installed.write().remove(id);
        if let Some(bus) = &self.event_bus {
            bus.publish("package.uninstalled", id.to_string());
        }
        Ok(())
    }

    /// Update trust scores for a package version.
    pub fn update_trust(&self, id: &str, version: &crate::version::Version, trust: TrustScore) -> Result<(), PackageStoreError> {
        let mut packages = self.packages.write();
        let versions = packages
            .get_mut(id)
            .ok_or_else(|| PackageStoreError::NotFound(id.to_string()))?;
        let pkg = versions
            .iter_mut()
            .find(|p| &p.manifest.version == version)
            .ok_or_else(|| PackageStoreError::VersionNotFound(id.to_string(), version.to_string()))?;
        pkg.trust = trust;
        Ok(())
    }

    /// Search packages by free-text query (matches ID, name, description, tags).
    /// Only returns the latest version of each matching package.
    pub fn search(&self, query: &str) -> Vec<Package> {
        let q = query.to_lowercase();
        let packages = self.packages.read();
        packages
            .values()
            .filter_map(|v| v.first())
            .filter(|p| {
                let m = &p.manifest;
                q.is_empty()
                    || m.id.to_lowercase().contains(&q)
                    || m.name.to_lowercase().contains(&q)
                    || m.description.to_lowercase().contains(&q)
                    || m.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .cloned()
            .collect()
    }

    /// List all packages (latest version of each).
    pub fn list(&self) -> Vec<Package> {
        self.packages
            .read()
            .values()
            .filter_map(|v| v.first().cloned())
            .collect()
    }

    /// List packages filtered by visibility.
    pub fn list_by_visibility(&self, vis: Visibility) -> Vec<Package> {
        self.list()
            .into_iter()
            .filter(|p| p.manifest.visibility == vis)
            .collect()
    }

    /// List all installed packages.
    pub fn list_installed(&self) -> Vec<Package> {
        let installed = self.installed.read();
        let packages = self.packages.read();
        installed
            .iter()
            .filter_map(|(id, ver)| {
                packages
                    .get(id)
                    .and_then(|v| v.iter().find(|p| p.manifest.version.to_string() == *ver).cloned())
            })
            .collect()
    }

    /// Insert a pre-built package directly (bypasses the normal publish flow).
    /// Used by the persistence layer to restore state from SQLite on startup.
    /// Does NOT emit events.
    pub fn insert_raw(&self, package: Package) {
        let id = package.manifest.id.clone();
        let mut packages = self.packages.write();
        let versions = packages.entry(id).or_default();
        versions.push(package);
        versions.sort_by(|a, b| b.manifest.version.cmp(&a.manifest.version));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{PackageBilling, PackageType, ResourceLimits, Visibility};
    use crate::version::{Version, VersionRange};

    fn sample_manifest(id: &str, version: Version) -> PackageManifest {
        PackageManifest {
            id: id.to_string(),
            name: format!("{} package", id),
            version,
            package_type: PackageType::Module,
            owner_public_key: "00".repeat(32),
            owner_name: "test".to_string(),
            capabilities: vec!["nxp.command.execute".to_string()],
            resource_limits: ResourceLimits::default(),
            dependencies: vec![],
            nxp_capabilities: vec!["quic".to_string()],
            core_compatibility: VersionRange::Caret(Version::new(0, 1, 0)),
            billing: PackageBilling::Free,
            visibility: Visibility::Public,
            signature: "00".repeat(64),
            description: "test description".to_string(),
            readme: "# test".to_string(),
            tags: vec!["test".to_string()],
        }
    }

    #[test]
    fn publish_and_get_latest() {
        let store = PackageStore::new();
        let m1 = sample_manifest("com.test.foo", Version::new(1, 0, 0));
        store.publish(m1).unwrap();
        let m2 = sample_manifest("com.test.foo", Version::new(2, 0, 0));
        store.publish(m2).unwrap();
        let latest = store.get_latest("com.test.foo").unwrap();
        assert_eq!(latest.manifest.version, Version::new(2, 0, 0));
        assert_eq!(store.get_all_versions("com.test.foo").len(), 2);
    }

    #[test]
    fn duplicate_version_rejected() {
        let store = PackageStore::new();
        let m = sample_manifest("com.test.foo", Version::new(1, 0, 0));
        store.publish(m.clone()).unwrap();
        let err = store.publish(m).unwrap_err();
        assert!(matches!(err, PackageStoreError::AlreadyExists(_)));
    }

    #[test]
    fn mark_installed_increments_counts() {
        let store = PackageStore::new();
        let m = sample_manifest("com.test.foo", Version::new(1, 0, 0));
        store.publish(m).unwrap();
        store
            .mark_installed("com.test.foo", &Version::new(1, 0, 0))
            .unwrap();
        let pkg = store.get_latest("com.test.foo").unwrap();
        assert!(pkg.installed);
        assert_eq!(pkg.install_count, 1);
        assert_eq!(pkg.active_install_count, 1);
        assert_eq!(store.installed_count(), 1);
    }

    #[test]
    fn mark_uninstalled_decrements_active() {
        let store = PackageStore::new();
        let m = sample_manifest("com.test.foo", Version::new(1, 0, 0));
        store.publish(m).unwrap();
        store
            .mark_installed("com.test.foo", &Version::new(1, 0, 0))
            .unwrap();
        store.mark_uninstalled("com.test.foo").unwrap();
        let pkg = store.get_latest("com.test.foo").unwrap();
        assert!(!pkg.installed);
        assert_eq!(pkg.install_count, 1); // cumulative
        assert_eq!(pkg.active_install_count, 0);
        assert_eq!(store.installed_count(), 0);
    }

    #[test]
    fn search_matches_id_name_tags() {
        let store = PackageStore::new();
        store.publish(sample_manifest("com.nexora.auth", Version::new(1, 0, 0))).unwrap();
        store.publish(sample_manifest("com.nexora.billing", Version::new(1, 0, 0))).unwrap();
        let r1 = store.search("auth");
        assert_eq!(r1.len(), 1);
        let r2 = store.search("nexora");
        assert_eq!(r2.len(), 2);
        let r3 = store.search("test");
        assert_eq!(r3.len(), 2); // description matches "test description"
        let r4 = store.search("");
        assert_eq!(r4.len(), 2);
    }

    #[test]
    fn list_by_visibility() {
        let store = PackageStore::new();
        let mut m1 = sample_manifest("pub", Version::new(1, 0, 0));
        m1.visibility = Visibility::Public;
        let mut m2 = sample_manifest("priv", Version::new(1, 0, 0));
        m2.visibility = Visibility::Private;
        store.publish(m1).unwrap();
        store.publish(m2).unwrap();
        assert_eq!(store.list_by_visibility(Visibility::Public).len(), 1);
        assert_eq!(store.list_by_visibility(Visibility::Private).len(), 1);
    }

    #[test]
    fn trust_score_aggregate() {
        let t = TrustScore {
            security: 100,
            performance: 100,
            stability: 100,
            community_rating: 5.0,
            enterprise_rating: 5.0,
        };
        assert_eq!(t.aggregate(), 100);
        let t = TrustScore {
            security: 0,
            performance: 0,
            stability: 0,
            community_rating: 0.0,
            enterprise_rating: 0.0,
        };
        assert_eq!(t.aggregate(), 0);
    }

    #[test]
    fn low_trust_detection() {
        let t = TrustScore {
            security: 40,
            ..Default::default()
        };
        assert!(t.is_low_trust());
        let t = TrustScore {
            security: 60,
            ..Default::default()
        };
        assert!(!t.is_low_trust());
    }

    #[test]
    fn update_trust_works() {
        let store = PackageStore::new();
        store.publish(sample_manifest("com.test.foo", Version::new(1, 0, 0))).unwrap();
        let new_trust = TrustScore {
            security: 95,
            performance: 90,
            stability: 85,
            community_rating: 4.5,
            enterprise_rating: 4.0,
        };
        store
            .update_trust("com.test.foo", &Version::new(1, 0, 0), new_trust.clone())
            .unwrap();
        let pkg = store.get_latest("com.test.foo").unwrap();
        assert_eq!(pkg.trust, new_trust);
    }
}
