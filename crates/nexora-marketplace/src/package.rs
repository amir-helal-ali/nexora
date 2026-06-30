//! Package model — manifest, types, billing, visibility.
//!
//! See Nexora Engineering Specification, Part 5 (PACKAGE MODEL).
//! Every package is identified by a unique ID and includes a manifest with
//! all metadata required for verification, distribution, and monetization.

use crate::version::Version;
use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;

/// Unique package identifier (slug-style, e.g. `com.nexora.auth`).
pub type PackageId = String;

/// Package type. See RFC §4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageType {
    /// Full system component (Auth, Billing, CRM, ERP, AI Orchestrator).
    Module,
    /// Extends a module without modifying core logic (sandboxed).
    Plugin,
    /// Autonomous system executing tasks via NXP (deferred — Part 11).
    AiAgent,
    /// Prebuilt system (SaaS starter, dashboard, full-stack kit).
    Template,
    /// Deployable runtime (DB, worker, API, microservice).
    Service,
    /// Workflow-based logic (CI/CD, billing automation, deployment).
    Automation,
}

impl fmt::Display for PackageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Module => f.write_str("module"),
            Self::Plugin => f.write_str("plugin"),
            Self::AiAgent => f.write_str("ai_agent"),
            Self::Template => f.write_str("template"),
            Self::Service => f.write_str("service"),
            Self::Automation => f.write_str("automation"),
        }
    }
}

/// Distribution visibility. See RFC §10.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Anyone can install.
    Public,
    /// Only the owner.
    Private,
    /// Only members of a specific organization.
    OrganizationOnly,
    /// Only enterprise customers.
    EnterpriseOnly,
    /// Only specific geographic regions.
    RegionRestricted,
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Public => f.write_str("public"),
            Self::Private => f.write_str("private"),
            Self::OrganizationOnly => f.write_str("org_only"),
            Self::EnterpriseOnly => f.write_str("enterprise_only"),
            Self::RegionRestricted => f.write_str("region_restricted"),
        }
    }
}

/// Billing model. See RFC §8.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "params", rename_all = "snake_case")]
pub enum PackageBilling {
    /// One-time purchase. `params` = price in minor units (cents).
    OneTime {
        /// Price in minor units (e.g. cents).
        price_minor: u64,
        /// ISO 4217 currency code (e.g. "USD").
        currency: String,
    },
    /// Recurring subscription.
    Subscription {
        /// Price per period in minor units.
        price_minor: u64,
        /// ISO 4217 currency code.
        currency: String,
        /// Period in seconds (e.g. 2592000 for monthly).
        period_seconds: u64,
    },
    /// Usage-based (per NXP command / per event).
    UsageBased {
        /// Price per 1000 operations in minor units.
        price_per_1k_minor: u64,
        /// ISO 4217 currency code.
        currency: String,
    },
    /// Enterprise licensing (custom terms).
    Enterprise {
        /// Contact email for sales.
        contact: String,
    },
    /// Free.
    Free,
}

impl fmt::Display for PackageBilling {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OneTime { price_minor, currency } => {
                write!(f, "{} {} (one-time)", price_minor, currency)
            }
            Self::Subscription { price_minor, currency, period_seconds } => {
                let days = period_seconds / 86400;
                write!(f, "{} {} / {}d", price_minor, currency, days)
            }
            Self::UsageBased { price_per_1k_minor, currency } => {
                write!(f, "{} {} / 1k ops", price_per_1k_minor, currency)
            }
            Self::Enterprise { contact } => write!(f, "enterprise ({})", contact),
            Self::Free => f.write_str("free"),
        }
    }
}

/// Resource limits declared by the package.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Max CPU percent.
    pub max_cpu_percent: u32,
    /// Max memory MB.
    pub max_memory_mb: u32,
    /// Max NXP commands per second.
    pub max_commands_per_sec: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_percent: 25,
            max_memory_mb: 256,
            max_commands_per_sec: 100,
        }
    }
}

/// The package manifest. Required metadata for every package.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackageManifest {
    /// Unique package ID (e.g. `com.nexora.auth`).
    pub id: PackageId,
    /// Human-readable name.
    pub name: String,
    /// Semantic version.
    pub version: Version,
    /// Package type.
    pub package_type: PackageType,
    /// Owner's Ed25519 public key (32 bytes, hex).
    pub owner_public_key: String,
    /// Owner display name.
    pub owner_name: String,
    /// Capabilities declared (e.g. `nxp.command.execute`, `event.publish`).
    pub capabilities: Vec<String>,
    /// Resource limits.
    pub resource_limits: ResourceLimits,
    /// Dependencies (other packages + version ranges).
    pub dependencies: Vec<crate::dependency::Dependency>,
    /// NXP capabilities required.
    pub nxp_capabilities: Vec<String>,
    /// Compatibility matrix (Nexora Core version range).
    pub core_compatibility: crate::version::VersionRange,
    /// Billing model.
    pub billing: PackageBilling,
    /// Visibility.
    pub visibility: Visibility,
    /// Ed25519 signature over canonical manifest bytes (64 bytes, hex).
    pub signature: String,
    /// Short description.
    pub description: String,
    /// Long-form readme (Markdown).
    pub readme: String,
    /// Tags for search.
    pub tags: Vec<String>,
}

/// A registered package (manifest + state).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Package {
    /// Manifest.
    pub manifest: PackageManifest,
    /// SHA-256 integrity hash (hex).
    pub integrity_hash: String,
    /// When the package was published (unix nanos).
    pub published_at: i64,
    /// Install count.
    pub install_count: u64,
    /// Active install count.
    pub active_install_count: u64,
    /// Trust scores.
    pub trust: crate::store::TrustScore,
    /// Whether the package is currently installed in this Core.
    pub installed: bool,
}

impl Package {
    /// Construct a new package from a manifest. Computes the integrity hash
    /// and initializes counters.
    pub fn from_manifest(manifest: PackageManifest) -> Self {
        let integrity_hash = compute_integrity_hash(&manifest);
        Self {
            manifest,
            integrity_hash,
            published_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            install_count: 0,
            active_install_count: 0,
            trust: crate::store::TrustScore::default(),
            installed: false,
        }
    }
}

/// Compute the SHA-256 integrity hash of a manifest. The hash is taken over
/// the canonical MessagePack encoding of the manifest with the `signature`
/// field blanked out (so the signature can sign the hash).
pub fn compute_integrity_hash(manifest: &PackageManifest) -> String {
    let mut canonical = manifest.clone();
    canonical.signature = String::new();
    let bytes = rmp_serde::to_vec_named(&canonical).unwrap_or_default();
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency::Dependency;
    use crate::version::VersionRange;

    fn sample_manifest(id: &str) -> PackageManifest {
        PackageManifest {
            id: id.to_string(),
            name: format!("{} package", id),
            version: Version::new(0, 1, 0),
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
            description: "test".to_string(),
            readme: "# test".to_string(),
            tags: vec!["test".to_string()],
        }
    }

    #[test]
    fn from_manifest_initializes_state() {
        let m = sample_manifest("com.test.foo");
        let p = Package::from_manifest(m);
        assert_eq!(p.manifest.id, "com.test.foo");
        assert_eq!(p.integrity_hash.len(), 64);
        assert_eq!(p.install_count, 0);
        assert!(!p.installed);
    }

    #[test]
    fn integrity_hash_stable_for_same_manifest() {
        let m1 = sample_manifest("com.test.foo");
        let m2 = sample_manifest("com.test.foo");
        let p1 = Package::from_manifest(m1);
        let p2 = Package::from_manifest(m2);
        assert_eq!(p1.integrity_hash, p2.integrity_hash);
    }

    #[test]
    fn integrity_hash_changes_with_content() {
        let mut m1 = sample_manifest("com.test.foo");
        let m2 = sample_manifest("com.test.foo");
        m1.description = "different".to_string();
        let p1 = Package::from_manifest(m1);
        let p2 = Package::from_manifest(m2);
        assert_ne!(p1.integrity_hash, p2.integrity_hash);
    }

    #[test]
    fn billing_display() {
        let b = PackageBilling::Free;
        assert_eq!(b.to_string(), "free");
        let b = PackageBilling::OneTime { price_minor: 1999, currency: "USD".into() };
        assert_eq!(b.to_string(), "1999 USD (one-time)");
        let b = PackageBilling::Subscription {
            price_minor: 999,
            currency: "USD".into(),
            period_seconds: 2592000,
        };
        assert_eq!(b.to_string(), "999 USD / 30d");
    }

    #[test]
    fn package_type_display() {
        assert_eq!(PackageType::Module.to_string(), "module");
        assert_eq!(PackageType::AiAgent.to_string(), "ai_agent");
    }

    #[test]
    fn visibility_display() {
        assert_eq!(Visibility::Public.to_string(), "public");
        assert_eq!(Visibility::EnterpriseOnly.to_string(), "enterprise_only");
    }

    #[test]
    fn manifest_serializes_with_dependencies() {
        let mut m = sample_manifest("com.test.foo");
        m.dependencies = vec![Dependency::new("com.nexora.core".into(), "^0.1.0").unwrap()];
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("com.nexora.core"));
        let parsed: PackageManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.dependencies.len(), 1);
    }
}
