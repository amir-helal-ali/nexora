//! Auto-update system for installed packages.
//!
//! See Nexora Engineering Specification, Part 5 (AUTO-UPDATE SYSTEM).
//! Packages may support:
//! - Auto-update (latest compatible version)
//! - Scheduled update (window-based)
//! - Manual update approval
//! - Rollback to any prior version
//!
//! Updates always run through the same 13-step installation pipeline.

use crate::install::{InstallError, InstallPipeline, InstallReport};
use crate::package::PackageId;
use crate::store::PackageStore;
use crate::version::Version;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use time::OffsetDateTime;

/// Update policy for a package.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePolicy {
    /// Automatically update to the latest compatible version.
    Auto,
    /// Manual approval required before updating.
    Manual,
    /// Updates disabled (frozen at current version).
    Disabled,
}

impl fmt::Display for UpdatePolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::Manual => f.write_str("manual"),
            Self::Disabled => f.write_str("disabled"),
        }
    }
}

/// An available update (newer version found).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AvailableUpdate {
    /// Package ID.
    pub package_id: PackageId,
    /// Currently installed version.
    pub current_version: Version,
    /// Available newer version.
    pub available_version: Version,
    /// Whether the update is compatible (same major version).
    pub compatible: bool,
}

/// Result of an update check.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    /// Packages that have updates available.
    pub updates: Vec<AvailableUpdate>,
    /// When the check ran (unix nanos).
    pub checked_at: i64,
}

/// Result of a rollback.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RollbackResult {
    /// Package ID.
    pub package_id: PackageId,
    /// Version rolled back from.
    pub from_version: Version,
    /// Version rolled back to.
    pub to_version: Version,
    /// Whether the rollback succeeded.
    pub success: bool,
    /// Install report (if the rollback ran the pipeline).
    pub report: Option<InstallReport>,
    /// Error message (if failed).
    pub error: Option<String>,
}

/// Error from update operations.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// Package not installed.
    #[error("package not installed: {0}")]
    NotInstalled(PackageId),
    /// No versions available to update to.
    #[error("no newer version available for {0}")]
    NoUpdateAvailable(PackageId),
    /// Update policy is disabled.
    #[error("updates disabled for {0}")]
    Disabled(PackageId),
    /// Manual approval required.
    #[error("manual approval required for {0}")]
    ManualApprovalRequired(PackageId),
    /// Install pipeline failed.
    #[error("install failed: {0}")]
    InstallFailed(#[from] InstallError),
    /// No previous version to roll back to.
    #[error("no previous version to roll back to for {0}")]
    NoPreviousVersion(PackageId),
    /// Target version not found.
    #[error("version {version} not found for {package_id}")]
    VersionNotFound {
        /// Package ID.
        package_id: PackageId,
        /// Requested version.
        version: Version,
    },
}

/// The auto-update manager. Tracks update policies per package and runs
/// update checks + rollbacks.
pub struct UpdateManager {
    pipeline: InstallPipeline,
    policies: RwLock<HashMap<PackageId, UpdatePolicy>>,
    event_bus: Option<Arc<nexora_core::EventBus>>,
}

impl fmt::Debug for UpdateManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.policies.read().len();
        f.debug_struct("UpdateManager")
            .field("tracked_policies", &count)
            .finish()
    }
}

impl Default for UpdateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateManager {
    /// Construct a new update manager.
    pub fn new() -> Self {
        Self {
            pipeline: InstallPipeline::new(),
            policies: RwLock::new(HashMap::new()),
            event_bus: None,
        }
    }

    /// Attach an EventBus for event publishing.
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Set the update policy for a package.
    pub fn set_policy(&self, package_id: &str, policy: UpdatePolicy) {
        self.policies
            .write()
            .insert(package_id.to_string(), policy);
    }

    /// Get the update policy for a package (defaults to Manual if unset).
    pub fn get_policy(&self, package_id: &str) -> UpdatePolicy {
        self.policies
            .read()
            .get(package_id)
            .copied()
            .unwrap_or(UpdatePolicy::Manual)
    }

    /// Check all installed packages for available updates.
    /// Returns a list of packages with newer versions available.
    pub fn check_updates(&self, store: &PackageStore) -> UpdateCheckResult {
        let installed = store.list_installed();
        let mut updates = Vec::new();

        for pkg in &installed {
            let current = &pkg.manifest.version;
            // Find the latest published version.
            if let Some(latest) = store.get_latest(&pkg.manifest.id) {
                if latest.manifest.version > *current {
                    let compatible = latest.manifest.version.is_compatible_with(current);
                    updates.push(AvailableUpdate {
                        package_id: pkg.manifest.id.clone(),
                        current_version: *current,
                        available_version: latest.manifest.version,
                        compatible,
                    });
                    // Emit event.
                    self.emit(
                        "package.update_available",
                        &format!(
                            "{}@{}→{}",
                            pkg.manifest.id, current, latest.manifest.version
                        ),
                    );
                }
            }
        }

        UpdateCheckResult {
            updates,
            checked_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
        }
    }

    /// Update a package to its latest compatible version. Runs the full
    /// 13-step installation pipeline for the new version.
    pub fn update_package(
        &self,
        store: &PackageStore,
        package_id: &str,
    ) -> Result<InstallReport, UpdateError> {
        // Check policy.
        let policy = self.get_policy(package_id);
        if policy == UpdatePolicy::Disabled {
            return Err(UpdateError::Disabled(package_id.to_string()));
        }

        // Find the currently installed version.
        let installed = store
            .list_installed()
            .into_iter()
            .find(|p| p.manifest.id == package_id)
            .ok_or_else(|| UpdateError::NotInstalled(package_id.to_string()))?;
        let current = installed.manifest.version;

        // Find the latest version.
        let latest = store.get_latest(package_id).ok_or_else(|| {
            UpdateError::VersionNotFound {
                package_id: package_id.to_string(),
                version: current,
            }
        })?;

        if latest.manifest.version <= current {
            return Err(UpdateError::NoUpdateAvailable(package_id.to_string()));
        }

        // Uninstall the current version first.
        let _ = store.mark_uninstalled(package_id);

        // Run the install pipeline for the new version.
        let report = self
            .pipeline
            .run(store, package_id, &latest.manifest.version)
            .map_err(UpdateError::InstallFailed)?;

        // Emit event.
        self.emit(
            "package.updated",
            &format!(
                "{}@{}→{}",
                package_id, current, latest.manifest.version
            ),
        );

        Ok(report)
    }

    /// Roll back a package to a specific previous version.
    pub fn rollback(
        &self,
        store: &PackageStore,
        package_id: &str,
        target_version: &Version,
    ) -> Result<RollbackResult, UpdateError> {
        // Find the currently installed version.
        let installed = store
            .list_installed()
            .into_iter()
            .find(|p| p.manifest.id == package_id)
            .ok_or_else(|| UpdateError::NotInstalled(package_id.to_string()))?;
        let current = installed.manifest.version;

        // Verify the target version exists.
        let target_pkg = store
            .get_version(package_id, target_version)
            .ok_or_else(|| UpdateError::VersionNotFound {
                package_id: package_id.to_string(),
                version: *target_version,
            })?;

        if *target_version >= current {
            return Err(UpdateError::NoPreviousVersion(package_id.to_string()));
        }

        // Uninstall current.
        let _ = store.mark_uninstalled(package_id);

        // Install the target version.
        let report_result = self
            .pipeline
            .run(store, package_id, &target_pkg.manifest.version);

        let (success, report, error) = match report_result {
            Ok(r) => (true, Some(r), None),
            Err(e) => (false, None, Some(e.to_string())),
        };

        // Emit event.
        self.emit(
            "package.rolled_back",
            &format!(
                "{}@{}→{}",
                package_id, current, target_version
            ),
        );

        Ok(RollbackResult {
            package_id: package_id.to_string(),
            from_version: current,
            to_version: *target_version,
            success,
            report,
            error,
        })
    }

    /// Process all auto-update packages. Returns the list of update reports.
    pub fn process_auto_updates(&self, store: &PackageStore) -> Vec<Result<InstallReport, UpdateError>> {
        let check = self.check_updates(store);
        let mut results = Vec::new();

        for update in &check.updates {
            // Only auto-update compatible versions.
            if !update.compatible {
                continue;
            }
            // Check policy.
            if self.get_policy(&update.package_id) != UpdatePolicy::Auto {
                continue;
            }
            // Attempt the update.
            let result = self.update_package(store, &update.package_id);
            results.push(result);
        }

        results
    }

    fn emit(&self, name: &str, payload: &str) {
        if let Some(bus) = &self.event_bus {
            bus.publish(name, payload.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{
        PackageBilling, PackageManifest, PackageType, ResourceLimits, Visibility,
    };
    use crate::version::VersionRange;
    use crate::signature;

    fn signed_manifest(id: &str, version: Version) -> PackageManifest {
        let mut m = PackageManifest {
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
            signature: "ff".repeat(64),
            description: "test".to_string(),
            readme: "# test".to_string(),
            tags: vec![],
        };
        // Sign it so the install pipeline accepts it.
        let signing = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let verifying = signing.verifying_key();
        m.owner_public_key = hex::encode(verifying.to_bytes());
        signature::sign_manifest(&mut m, &signing);
        m
    }

    fn setup_store() -> PackageStore {
        let bus = Arc::new(nexora_core::EventBus::new());
        PackageStore::new().with_event_bus(bus)
    }

    #[test]
    fn check_updates_finds_newer_version() {
        let store = setup_store();
        // Publish v1.0.0 and install it.
        store.publish(signed_manifest("com.test.foo", Version::new(1, 0, 0))).unwrap();
        store
            .mark_installed("com.test.foo", &Version::new(1, 0, 0))
            .unwrap();
        // Publish v1.1.0 (not installed).
        store.publish(signed_manifest("com.test.foo", Version::new(1, 1, 0))).unwrap();

        let mgr = UpdateManager::new();
        let result = mgr.check_updates(&store);
        assert_eq!(result.updates.len(), 1);
        assert_eq!(result.updates[0].current_version, Version::new(1, 0, 0));
        assert_eq!(result.updates[0].available_version, Version::new(1, 1, 0));
        assert!(result.updates[0].compatible);
    }

    #[test]
    fn check_updates_no_newer_version() {
        let store = setup_store();
        store.publish(signed_manifest("com.test.foo", Version::new(1, 0, 0))).unwrap();
        store
            .mark_installed("com.test.foo", &Version::new(1, 0, 0))
            .unwrap();

        let mgr = UpdateManager::new();
        let result = mgr.check_updates(&store);
        assert_eq!(result.updates.len(), 0);
    }

    #[test]
    fn check_updates_detects_incompatible() {
        let store = setup_store();
        store.publish(signed_manifest("com.test.foo", Version::new(1, 0, 0))).unwrap();
        store
            .mark_installed("com.test.foo", &Version::new(1, 0, 0))
            .unwrap();
        // Publish v2.0.0 (breaking).
        store.publish(signed_manifest("com.test.foo", Version::new(2, 0, 0))).unwrap();

        let mgr = UpdateManager::new();
        let result = mgr.check_updates(&store);
        assert_eq!(result.updates.len(), 1);
        assert!(!result.updates[0].compatible); // major version change
    }

    #[test]
    fn update_package_succeeds() {
        let store = setup_store();
        store.publish(signed_manifest("com.test.foo", Version::new(1, 0, 0))).unwrap();
        store
            .mark_installed("com.test.foo", &Version::new(1, 0, 0))
            .unwrap();
        store.publish(signed_manifest("com.test.foo", Version::new(1, 1, 0))).unwrap();

        let mgr = UpdateManager::new();
        mgr.set_policy("com.test.foo", UpdatePolicy::Auto);
        let report = mgr.update_package(&store, "com.test.foo").unwrap();
        assert!(report.success);

        // Verify v1.1.0 is now installed.
        let installed = store.list_installed();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].manifest.version, Version::new(1, 1, 0));
    }

    #[test]
    fn update_disabled_rejected() {
        let store = setup_store();
        store.publish(signed_manifest("com.test.foo", Version::new(1, 0, 0))).unwrap();
        store
            .mark_installed("com.test.foo", &Version::new(1, 0, 0))
            .unwrap();
        store.publish(signed_manifest("com.test.foo", Version::new(1, 1, 0))).unwrap();

        let mgr = UpdateManager::new();
        mgr.set_policy("com.test.foo", UpdatePolicy::Disabled);
        let err = mgr.update_package(&store, "com.test.foo").unwrap_err();
        assert!(matches!(err, UpdateError::Disabled(_)));
    }

    #[test]
    fn update_no_newer_version_rejected() {
        let store = setup_store();
        store.publish(signed_manifest("com.test.foo", Version::new(1, 0, 0))).unwrap();
        store
            .mark_installed("com.test.foo", &Version::new(1, 0, 0))
            .unwrap();

        let mgr = UpdateManager::new();
        let err = mgr.update_package(&store, "com.test.foo").unwrap_err();
        assert!(matches!(err, UpdateError::NoUpdateAvailable(_)));
    }

    #[test]
    fn rollback_to_previous_version() {
        let store = setup_store();
        store.publish(signed_manifest("com.test.foo", Version::new(1, 0, 0))).unwrap();
        store.publish(signed_manifest("com.test.foo", Version::new(1, 1, 0))).unwrap();
        // Install v1.1.0.
        store
            .mark_installed("com.test.foo", &Version::new(1, 1, 0))
            .unwrap();

        let mgr = UpdateManager::new();
        let result = mgr.rollback(&store, "com.test.foo", &Version::new(1, 0, 0)).unwrap();
        assert!(result.success);
        assert_eq!(result.from_version, Version::new(1, 1, 0));
        assert_eq!(result.to_version, Version::new(1, 0, 0));

        // Verify v1.0.0 is now installed.
        let installed = store.list_installed();
        assert_eq!(installed[0].manifest.version, Version::new(1, 0, 0));
    }

    #[test]
    fn rollback_to_same_or_newer_rejected() {
        let store = setup_store();
        store.publish(signed_manifest("com.test.foo", Version::new(1, 0, 0))).unwrap();
        store.publish(signed_manifest("com.test.foo", Version::new(1, 1, 0))).unwrap();
        store
            .mark_installed("com.test.foo", &Version::new(1, 0, 0))
            .unwrap();

        let mgr = UpdateManager::new();
        let err = mgr
            .rollback(&store, "com.test.foo", &Version::new(1, 1, 0))
            .unwrap_err();
        assert!(matches!(err, UpdateError::NoPreviousVersion(_)));
    }

    #[test]
    fn rollback_unknown_version_rejected() {
        let store = setup_store();
        store.publish(signed_manifest("com.test.foo", Version::new(1, 0, 0))).unwrap();
        store
            .mark_installed("com.test.foo", &Version::new(1, 0, 0))
            .unwrap();

        let mgr = UpdateManager::new();
        let err = mgr
            .rollback(&store, "com.test.foo", &Version::new(0, 5, 0))
            .unwrap_err();
        assert!(matches!(err, UpdateError::VersionNotFound { .. }));
    }

    #[test]
    fn process_auto_updates_only_updates_auto_policy() {
        let store = setup_store();
        // Package A: auto policy, has update.
        store.publish(signed_manifest("com.test.a", Version::new(1, 0, 0))).unwrap();
        store.mark_installed("com.test.a", &Version::new(1, 0, 0)).unwrap();
        store.publish(signed_manifest("com.test.a", Version::new(1, 1, 0))).unwrap();
        // Package B: manual policy, has update.
        store.publish(signed_manifest("com.test.b", Version::new(1, 0, 0))).unwrap();
        store.mark_installed("com.test.b", &Version::new(1, 0, 0)).unwrap();
        store.publish(signed_manifest("com.test.b", Version::new(1, 1, 0))).unwrap();

        let mgr = UpdateManager::new();
        mgr.set_policy("com.test.a", UpdatePolicy::Auto);
        mgr.set_policy("com.test.b", UpdatePolicy::Manual);

        let results = mgr.process_auto_updates(&store);
        assert_eq!(results.len(), 1); // Only package A was auto-updated.
        assert!(results[0].is_ok());

        // Verify A is at v1.1.0, B is still at v1.0.0.
        let installed = store.list_installed();
        let a = installed.iter().find(|p| p.manifest.id == "com.test.a").unwrap();
        let b = installed.iter().find(|p| p.manifest.id == "com.test.b").unwrap();
        assert_eq!(a.manifest.version, Version::new(1, 1, 0));
        assert_eq!(b.manifest.version, Version::new(1, 0, 0));
    }

    #[test]
    fn events_emitted() {
        let bus = Arc::new(nexora_core::EventBus::new());
        let store = PackageStore::new().with_event_bus(bus.clone());
        store.publish(signed_manifest("com.test.foo", Version::new(1, 0, 0))).unwrap();
        store.mark_installed("com.test.foo", &Version::new(1, 0, 0)).unwrap();
        store.publish(signed_manifest("com.test.foo", Version::new(1, 1, 0))).unwrap();

        let mgr = UpdateManager::new().with_event_bus(bus.clone());
        mgr.set_policy("com.test.foo", UpdatePolicy::Auto);

        // check_updates emits "package.update_available".
        mgr.check_updates(&store);
        // update_package emits "package.updated".
        mgr.update_package(&store, "com.test.foo").unwrap();

        let events = bus.replay_filtered(0, "package.");
        let names: Vec<&str> = events.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"package.update_available"));
        assert!(names.contains(&"package.updated"));
    }
}
