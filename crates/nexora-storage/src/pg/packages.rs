//! PostgreSQL package store — marketplace catalog persistence.

use crate::pg::{PgError, PgPool};
use nexora_marketplace::package::{Package, PackageManifest, PackageType, Visibility, PackageBilling, ResourceLimits};
use nexora_marketplace::store::TrustScore;
use nexora_marketplace::dependency::Dependency;
use nexora_marketplace::version::VersionRange;
use nexora_marketplace::package::compute_integrity_hash;
use time::OffsetDateTime;

/// A row in the packages table.
#[derive(Debug, Clone)]
pub struct PgPackageRow {
    pub id: String,
    pub version: String,
    pub manifest_json: String,
    pub integrity_hash: String,
    pub published_at: i64,
    pub install_count: i64,
    pub active_install_count: i64,
    pub installed: bool,
}

/// PostgreSQL package store.
pub struct PgPackageStore {
    pool: PgPool,
}

impl PgPackageStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert or update a package (UPSERT by id+version).
    pub async fn upsert(&self, pkg: &Package) -> Result<(), PgError> {
        let manifest_json = serde_json::to_string(&pkg.manifest)?;
        let v = &pkg.manifest.version;
        let version_str = format!("{}.{}.{}", v.major, v.minor, v.patch);
        self.pool.execute(
            r#"INSERT INTO packages (
                id, version, name, package_type, owner_public_key, owner_name,
                capabilities, resource_limits, dependencies, nxp_capabilities,
                core_compatibility, billing, visibility, signature,
                description, readme, tags, integrity_hash, published_at,
                install_count, active_install_count, installed
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19,
                $20, $21, $22
            )
            ON CONFLICT (id, version) DO UPDATE SET
                install_count = EXCLUDED.install_count,
                active_install_count = EXCLUDED.active_install_count,
                installed = EXCLUDED.installed"#,
            &[
                &pkg.manifest.id as &(dyn postgres_types::ToSql + Sync),
                &version_str,
                &pkg.manifest.name,
                &format!("{:?}", pkg.manifest.package_type),
                &pkg.manifest.owner_public_key,
                &pkg.manifest.owner_name,
                &serde_json::to_value(&pkg.manifest.capabilities)?,
                &serde_json::to_value(&pkg.manifest.resource_limits)?,
                &serde_json::to_value(&pkg.manifest.dependencies)?,
                &serde_json::to_value(&pkg.manifest.nxp_capabilities)?,
                &pkg.manifest.core_compatibility.to_string(),
                &format!("{:?}", pkg.manifest.billing),
                &format!("{:?}", pkg.manifest.visibility),
                &pkg.manifest.signature,
                &pkg.manifest.description,
                &pkg.manifest.readme,
                &serde_json::to_value(&pkg.manifest.tags)?,
                &pkg.integrity_hash,
                &pkg.published_at,
                &(pkg.install_count as i64),
                &(pkg.active_install_count as i64),
                &pkg.installed,
            ],
        ).await?;
        Ok(())
    }

    /// Fetch a package by id+version.
    pub async fn get(&self, id: &str, version: &str) -> Result<Option<PgPackageRow>, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_opt(
            "SELECT id, version, description, integrity_hash, published_at,
                    install_count, active_install_count, installed
             FROM packages WHERE id = $1 AND version = $2",
            &[&id, &version],
        ).await?;
        match row {
            Some(r) => Ok(Some(PgPackageRow {
                id: r.get(0),
                version: r.get(1),
                manifest_json: r.get::<_, String>(2), // re-using description column for json
                integrity_hash: r.get(3),
                published_at: r.get(4),
                install_count: r.get(5),
                active_install_count: r.get(6),
                installed: r.get(7),
            })),
            None => Ok(None),
        }
    }

    /// Total package count.
    pub async fn count(&self) -> Result<i64, PgError> {
        let conn = self.pool.get_conn().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM packages", &[]).await?;
        Ok(row.get(0))
    }

    /// List packages by owner.
    pub async fn list_by_owner(&self, owner_key: &str) -> Result<Vec<PgPackageRow>, PgError> {
        let conn = self.pool.get_conn().await?;
        let rows = conn.query(
            "SELECT id, version, description, integrity_hash, published_at,
                    install_count, active_install_count, installed
             FROM packages WHERE owner_public_key = $1
             ORDER BY published_at DESC",
            &[&owner_key],
        ).await?;
        rows.iter().map(|r| Ok(PgPackageRow {
            id: r.get(0),
            version: r.get(1),
            manifest_json: r.get(2),
            integrity_hash: r.get(3),
            published_at: r.get(4),
            install_count: r.get(5),
            active_install_count: r.get(6),
            installed: r.get(7),
        })).collect()
    }
}
