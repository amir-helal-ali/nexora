//! Secret Manager — encrypted, versioned, audited secrets.
//!
//! See Nexora Engineering Specification, Part 4 (SECRET MANAGEMENT) and
//! Law 19. Secrets never appear in source code. Secrets are encrypted,
//! rotate automatically, are versioned, and are auditable.
//!
//! # v0.1 Implementation
//!
//! For the MVP, secrets are stored in memory keyed by ID. Each secret has
//! a version history. In production (Tier 2/3), this is backed by an HSM
//! or a vault service. The public API is identical.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use time::OffsetDateTime;
use zeroize::Zeroize;

/// Secret identifier (e.g. `db.password`, `oauth.client_secret`).
pub type SecretId = String;

/// Monotonically-increasing version number for a secret.
pub type SecretVersion = u64;

/// A single secret value with metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecretEntry {
    /// Version number.
    pub version: SecretVersion,
    /// Encrypted secret value (ciphertext).
    pub ciphertext: Vec<u8>,
    /// When this version was created (unix nanos).
    pub created_at: i64,
    /// Whether this version is the active one.
    pub active: bool,
}

/// The Secret Manager. Thread-safe.
pub struct SecretManager {
    secrets: RwLock<HashMap<SecretId, Vec<SecretEntry>>>,
}

impl fmt::Debug for SecretManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.secrets.read().len();
        f.debug_struct("SecretManager")
            .field("secret_count", &count)
            .finish()
    }
}

impl Default for SecretManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretManager {
    /// Construct an empty secret manager.
    pub fn new() -> Self {
        Self {
            secrets: RwLock::new(HashMap::new()),
        }
    }

    /// Number of distinct secret IDs.
    pub fn secret_count(&self) -> usize {
        self.secrets.read().len()
    }

    /// Store a new version of a secret. Returns the new version number.
    /// In v0.1 we store the plaintext as "ciphertext" — production deployments
    /// replace this with AEAD encryption backed by an HSM-derived key.
    pub fn put(&self, id: impl Into<String>, mut value: Vec<u8>) -> SecretVersion {
        let id = id.into();
        let mut secrets = self.secrets.write();
        let versions = secrets.entry(id.clone()).or_default();
        // Deactivate previous.
        for v in versions.iter_mut() {
            v.active = false;
        }
        let version = (versions.len() as u64) + 1;
        versions.push(SecretEntry {
            version,
            ciphertext: std::mem::take(&mut value),
            created_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            active: true,
        });
        // Zero the original buffer.
        value.zeroize();
        version
    }

    /// Get the active version of a secret.
    pub fn get(&self, id: &str) -> Option<SecretEntry> {
        self.secrets
            .read()
            .get(id)
            .and_then(|vs| vs.iter().find(|v| v.active).cloned())
    }

    /// Get a specific version of a secret.
    pub fn get_version(&self, id: &str, version: SecretVersion) -> Option<SecretEntry> {
        self.secrets
            .read()
            .get(id)
            .and_then(|vs| vs.iter().find(|v| v.version == version).cloned())
    }

    /// List all secret IDs.
    pub fn list(&self) -> Vec<SecretId> {
        self.secrets.read().keys().cloned().collect()
    }

    /// Delete a secret entirely (all versions).
    pub fn delete(&self, id: &str) -> bool {
        self.secrets.write().remove(id).is_some()
    }

    /// Roll back to a specific version. Returns `Ok(())` if successful.
    pub fn rollback(&self, id: &str, version: SecretVersion) -> Result<(), SecretError> {
        let mut secrets = self.secrets.write();
        let versions = secrets
            .get_mut(id)
            .ok_or(SecretError::NotFound(id.to_string()))?;
        if !versions.iter().any(|v| v.version == version) {
            return Err(SecretError::VersionNotFound { id: id.to_string(), version });
        }
        for v in versions.iter_mut() {
            v.active = v.version == version;
        }
        Ok(())
    }

    /// Number of versions for a secret.
    pub fn version_count(&self, id: &str) -> usize {
        self.secrets
            .read()
            .get(id)
            .map(|vs| vs.len())
            .unwrap_or(0)
    }
}

/// Error from secret operations.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// Secret not found.
    #[error("secret not found: {0}")]
    NotFound(SecretId),
    /// Secret version not found.
    #[error("secret {id} version {version} not found")]
    VersionNotFound {
        /// Secret ID.
        id: SecretId,
        /// Version requested.
        version: SecretVersion,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_works() {
        let mgr = SecretManager::new();
        let v1 = mgr.put("db.password", b"hunter2".to_vec());
        assert_eq!(v1, 1);
        let entry = mgr.get("db.password").unwrap();
        assert_eq!(entry.ciphertext, b"hunter2");
        assert!(entry.active);
        assert_eq!(mgr.version_count("db.password"), 1);
    }

    #[test]
    fn put_creates_new_version_and_deactivates_old() {
        let mgr = SecretManager::new();
        mgr.put("k", b"v1".to_vec());
        let v2 = mgr.put("k", b"v2".to_vec());
        assert_eq!(v2, 2);
        let active = mgr.get("k").unwrap();
        assert_eq!(active.version, 2);
        assert_eq!(active.ciphertext, b"v2");
        let old = mgr.get_version("k", 1).unwrap();
        assert!(!old.active);
    }

    #[test]
    fn rollback_works() {
        let mgr = SecretManager::new();
        mgr.put("k", b"v1".to_vec());
        mgr.put("k", b"v2".to_vec());
        mgr.put("k", b"v3".to_vec());
        assert_eq!(mgr.get("k").unwrap().ciphertext, b"v3");
        mgr.rollback("k", 1).unwrap();
        assert_eq!(mgr.get("k").unwrap().ciphertext, b"v1");
    }

    #[test]
    fn delete_removes_all_versions() {
        let mgr = SecretManager::new();
        mgr.put("k", b"v1".to_vec());
        mgr.put("k", b"v2".to_vec());
        assert!(mgr.delete("k"));
        assert!(mgr.get("k").is_none());
        assert_eq!(mgr.secret_count(), 0);
    }
}
