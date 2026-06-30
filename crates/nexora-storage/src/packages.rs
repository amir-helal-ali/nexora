//! SQLite-backed package store.
//!
//! Persists packages (including all versions) to SQLite. Writes through on
//! every publish, install, uninstall, and trust update.

use crate::{Database, StorageError};
use nexora_marketplace::package::{Package, PackageManifest};
use nexora_marketplace::store::{PackageStoreError, TrustScore};
use nexora_marketplace::version::Version;
use std::sync::Arc;

/// SQLite-backed package store. Wraps the in-memory `PackageStore` and
/// writes through to SQLite on every mutation.
pub struct SqlitePackageStore {
    db: Database,
    bus: Option<Arc<nexora_core::EventBus>>,
}

impl std::fmt::Debug for SqlitePackageStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlitePackageStore")
            .field("db", &self.db)
            .finish_non_exhaustive()
    }
}

impl SqlitePackageStore {
    /// Construct a new SQLite-backed package store.
    pub fn new(db: Database) -> Self {
        Self { db, bus: None }
    }

    /// Attach an Event Bus.
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.bus = Some(bus);
        self
    }

    /// Publish a package. Writes to SQLite + in-memory store.
    pub fn publish(
        &self,
        mem: &nexora_marketplace::PackageStore,
        manifest: PackageManifest,
    ) -> Result<Package, PackageStoreError> {
        // Use the in-memory store's publish() (which computes integrity hash).
        let pkg = mem.publish(manifest)?;
        // Write through to SQLite.
        let manifest_json = serde_json::to_string(&pkg.manifest)
            .map_err(|e| PackageStoreError::NotFound(e.to_string()))?;
        let trust_json = serde_json::to_string(&pkg.trust)
            .map_err(|e| PackageStoreError::NotFound(e.to_string()))?;
        self.db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO packages (id, version, manifest_json, integrity_hash, published_at, install_count, active_install_count, installed, trust_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        pkg.manifest.id,
                        pkg.manifest.version.to_string(),
                        manifest_json,
                        pkg.integrity_hash,
                        pkg.published_at,
                        pkg.install_count,
                        pkg.active_install_count,
                        if pkg.installed { 1 } else { 0 },
                        trust_json,
                    ],
                )?;
                Ok(())
            })
            .map_err(|e| PackageStoreError::NotFound(e.to_string()))?;
        Ok(pkg)
    }

    /// Mark a package as installed. Updates SQLite + in-memory.
    pub fn mark_installed(
        &self,
        mem: &nexora_marketplace::PackageStore,
        id: &str,
        version: &Version,
    ) -> Result<(), PackageStoreError> {
        mem.mark_installed(id, version)?;
        self.db
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE packages SET install_count = install_count + 1, active_install_count = active_install_count + 1, installed = 1
                     WHERE id = ?1 AND version = ?2",
                    rusqlite::params![id, version.to_string()],
                )?;
                Ok(())
            })
            .map_err(|e| PackageStoreError::NotFound(e.to_string()))?;
        Ok(())
    }

    /// Mark a package as uninstalled. Updates SQLite + in-memory.
    pub fn mark_uninstalled(
        &self,
        mem: &nexora_marketplace::PackageStore,
        id: &str,
    ) -> Result<(), PackageStoreError> {
        mem.mark_uninstalled(id)?;
        self.db
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE packages SET active_install_count = MAX(0, active_install_count - 1), installed = 0
                     WHERE id = ?1",
                    rusqlite::params![id],
                )?;
                Ok(())
            })
            .map_err(|e| PackageStoreError::NotFound(e.to_string()))?;
        Ok(())
    }

    /// Update trust scores. Updates SQLite + in-memory.
    pub fn update_trust(
        &self,
        mem: &nexora_marketplace::PackageStore,
        id: &str,
        version: &Version,
        trust: TrustScore,
    ) -> Result<(), PackageStoreError> {
        mem.update_trust(id, version, trust.clone())?;
        let trust_json = serde_json::to_string(&trust)
            .map_err(|e| PackageStoreError::NotFound(e.to_string()))?;
        self.db
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE packages SET trust_json = ?1 WHERE id = ?2 AND version = ?3",
                    rusqlite::params![trust_json, id, version.to_string()],
                )?;
                Ok(())
            })
            .map_err(|e| PackageStoreError::NotFound(e.to_string()))?;
        Ok(())
    }

    /// Load all packages from SQLite into the in-memory store (call on startup).
    pub fn load_into(
        &self,
        mem: &nexora_marketplace::PackageStore,
    ) -> Result<usize, StorageError> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, version, manifest_json, integrity_hash, published_at, install_count, active_install_count, installed, trust_json
                 FROM packages",
            )?;
            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let version_str: String = row.get(1)?;
                let manifest_json: String = row.get(2)?;
                let integrity_hash: String = row.get(3)?;
                let published_at: i64 = row.get(4)?;
                let install_count: i64 = row.get(5)?;
                let active_install_count: i64 = row.get(6)?;
                let installed: i64 = row.get(7)?;
                let trust_json: String = row.get(8)?;
                Ok((id, version_str, manifest_json, integrity_hash, published_at, install_count, active_install_count, installed, trust_json))
            })?;

            let mut count = 0;
            for row_result in rows {
                let (_, _, manifest_json, integrity_hash, published_at, install_count, active_install_count, installed, trust_json) =
                    row_result?;
                let manifest: PackageManifest = serde_json::from_str(&manifest_json)?;
                let trust: TrustScore = serde_json::from_str(&trust_json)?;
                let pkg = Package {
                    manifest,
                    integrity_hash,
                    published_at,
                    install_count: install_count as u64,
                    active_install_count: active_install_count as u64,
                    trust,
                    installed: installed != 0,
                };
                // Insert directly using the in-memory store's insert_raw method.
                mem.insert_raw(pkg);
                count += 1;
            }
            Ok(count)
        })
    }

    /// Total package version count in SQLite.
    pub fn count(&self) -> Result<i64, StorageError> {
        self.db.with_conn(|conn| {
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM packages", [], |row| row.get(0))?;
            Ok(count)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_marketplace::package::{PackageBilling, PackageType, ResourceLimits, Visibility};
    use nexora_marketplace::version::{Version, VersionRange};

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
            signature: "ff".repeat(64),
            description: "test".to_string(),
            readme: "# test".to_string(),
            tags: vec!["test".to_string()],
        }
    }

    fn setup() -> (SqlitePackageStore, nexora_marketplace::PackageStore) {
        let db = Database::open_in_memory().unwrap();
        let bus = Arc::new(nexora_core::EventBus::new());
        let sql = SqlitePackageStore::new(db).with_event_bus(bus.clone());
        let mem = nexora_marketplace::PackageStore::new().with_event_bus(bus);
        (sql, mem)
    }

    #[test]
    fn publish_and_count() {
        let (sql, mem) = setup();
        assert_eq!(sql.count().unwrap(), 0);
        sql.publish(&mem, sample_manifest("com.test.a", Version::new(1, 0, 0))).unwrap();
        sql.publish(&mem, sample_manifest("com.test.b", Version::new(1, 0, 0))).unwrap();
        assert_eq!(sql.count().unwrap(), 2);
        assert_eq!(mem.package_count(), 2);
    }

    #[test]
    fn mark_installed_updates_both() {
        let (sql, mem) = setup();
        sql.publish(&mem, sample_manifest("com.test.a", Version::new(1, 0, 0))).unwrap();
        sql.mark_installed(&mem, "com.test.a", &Version::new(1, 0, 0)).unwrap();
        let pkg = mem.get_latest("com.test.a").unwrap();
        assert!(pkg.installed);
        assert_eq!(pkg.install_count, 1);
    }

    #[test]
    fn load_into_restores_packages() {
        let (sql1, mem1) = setup();
        sql1.publish(&mem1, sample_manifest("com.test.a", Version::new(1, 0, 0))).unwrap();
        sql1.publish(&mem1, sample_manifest("com.test.b", Version::new(2, 0, 0))).unwrap();

        // Simulate restart.
        let (_, mem2) = setup();
        let loaded = sql1.load_into(&mem2).unwrap();
        assert_eq!(loaded, 2);
        assert_eq!(mem2.package_count(), 2);
        assert!(mem2.get_latest("com.test.a").is_some());
        assert!(mem2.get_latest("com.test.b").is_some());
    }

    #[test]
    fn update_trust_persists() {
        let (sql, mem) = setup();
        sql.publish(&mem, sample_manifest("com.test.a", Version::new(1, 0, 0))).unwrap();
        let new_trust = TrustScore {
            security: 95,
            performance: 90,
            stability: 85,
            community_rating: 4.5,
            enterprise_rating: 4.0,
        };
        sql.update_trust(&mem, "com.test.a", &Version::new(1, 0, 0), new_trust.clone()).unwrap();
        let pkg = mem.get_latest("com.test.a").unwrap();
        assert_eq!(pkg.trust, new_trust);
    }
}
