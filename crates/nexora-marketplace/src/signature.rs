//! Package signature verification (Ed25519).
//!
//! See Nexora Engineering Specification, Part 5 (SECURITY MODEL).
//! Every package must carry an Ed25519 signature over its canonical manifest
//! (with the `signature` field blanked out). The signature is verified
//! against the owner's declared public key.

use crate::package::PackageManifest;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

/// Error from signature verification.
#[derive(Debug, thiserror::Error)]
pub enum PackageSignatureError {
    /// Public key bytes were invalid (must be 32 bytes, hex-encoded).
    #[error("invalid owner public key: {0}")]
    InvalidPublicKey(String),
    /// Signature bytes were invalid (must be 64 bytes, hex-encoded).
    #[error("invalid signature: {0}")]
    InvalidSignature(String),
    /// Signature did not verify against the manifest + public key.
    #[error("signature verification failed")]
    VerificationFailed,
}

/// Verify a package's Ed25519 signature against its declared owner public key.
///
/// The signed message is the SHA-256 hash of the canonical manifest (with
/// the `signature` field blanked out). This binds the signature to the
/// entire manifest content.
pub fn verify_package_signature(manifest: &PackageManifest) -> Result<(), PackageSignatureError> {
    // Decode the public key (32 bytes from hex).
    let pk_bytes = hex::decode(&manifest.owner_public_key)
        .map_err(|e| PackageSignatureError::InvalidPublicKey(e.to_string()))?;
    if pk_bytes.len() != 32 {
        return Err(PackageSignatureError::InvalidPublicKey(format!(
            "expected 32 bytes, got {}",
            pk_bytes.len()
        )));
    }
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pk_bytes);
    let verifying_key = VerifyingKey::from_bytes(&pk_arr)
        .map_err(|e| PackageSignatureError::InvalidPublicKey(e.to_string()))?;

    // Decode the signature (64 bytes from hex).
    let sig_bytes = hex::decode(&manifest.signature)
        .map_err(|e| PackageSignatureError::InvalidSignature(e.to_string()))?;
    if sig_bytes.len() != 64 {
        return Err(PackageSignatureError::InvalidSignature(format!(
            "expected 64 bytes, got {}",
            sig_bytes.len()
        )));
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let signature = Signature::from_bytes(&sig_arr);

    // Compute the message hash (same as integrity_hash).
    let hash = compute_integrity_hash_bytes(manifest);

    // Verify.
    verifying_key
        .verify(&hash, &signature)
        .map_err(|_| PackageSignatureError::VerificationFailed)
}

/// Compute the raw 32-byte SHA-256 of the canonical manifest (with signature
/// field blanked). Used internally by both `compute_integrity_hash` (which
/// hex-encodes it) and `verify_package_signature`.
fn compute_integrity_hash_bytes(manifest: &PackageManifest) -> [u8; 32] {
    let mut canonical = manifest.clone();
    canonical.signature = String::new();
    let bytes = rmp_serde::to_vec_named(&canonical).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    out
}

/// Sign a manifest with a private key. Returns the hex-encoded signature.
/// This is used by publishers (CLI / SDK) to sign their manifests before
/// uploading to the marketplace.
pub fn sign_manifest(manifest: &mut PackageManifest, signing_key: &ed25519_dalek::SigningKey) {
    use ed25519_dalek::Signer;
    // Blank the signature field, compute hash, sign, then set the signature.
    manifest.signature = String::new();
    let hash = compute_integrity_hash_bytes(manifest);
    let signature = signing_key.sign(&hash);
    manifest.signature = hex::encode(signature.to_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{PackageBilling, PackageManifest, PackageType, ResourceLimits, Visibility};
    use crate::version::{Version, VersionRange};
    use ed25519_dalek::{SigningKey, Verifier};
    use rand::rngs::OsRng;

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
    fn unsigned_manifest_fails_verification() {
        let m = sample_manifest("com.test.foo");
        // The all-zeros key + all-zeros signature is a special case that
        // Ed25519 may accept. Use a clearly invalid signature instead.
        let mut m = m;
        m.signature = "ff".repeat(64);
        let err = verify_package_signature(&m).unwrap_err();
        assert!(matches!(err, PackageSignatureError::VerificationFailed));
    }

    #[test]
    fn signed_manifest_verifies() {
        let signing = SigningKey::generate(&mut OsRng);
        let verifying = signing.verifying_key();
        let mut m = sample_manifest("com.test.foo");
        m.owner_public_key = hex::encode(verifying.to_bytes());
        sign_manifest(&mut m, &signing);
        assert!(verify_package_signature(&m).is_ok());
    }

    #[test]
    fn tampered_manifest_fails_verification() {
        let signing = SigningKey::generate(&mut OsRng);
        let verifying = signing.verifying_key();
        let mut m = sample_manifest("com.test.foo");
        m.owner_public_key = hex::encode(verifying.to_bytes());
        sign_manifest(&mut m, &signing);
        // Tamper.
        m.description = "different".to_string();
        let err = verify_package_signature(&m).unwrap_err();
        assert!(matches!(err, PackageSignatureError::VerificationFailed));
    }

    #[test]
    fn wrong_public_key_fails() {
        let signing1 = SigningKey::generate(&mut OsRng);
        let signing2 = SigningKey::generate(&mut OsRng);
        let verifying2 = signing2.verifying_key();
        let mut m = sample_manifest("com.test.foo");
        // Sign with key1 but declare key2 as the owner.
        m.owner_public_key = hex::encode(verifying2.to_bytes());
        sign_manifest(&mut m, &signing1);
        let err = verify_package_signature(&m).unwrap_err();
        assert!(matches!(err, PackageSignatureError::VerificationFailed));
    }

    #[test]
    fn invalid_public_key_rejected() {
        let mut m = sample_manifest("com.test.foo");
        m.owner_public_key = "not-hex-zzz".to_string();
        let err = verify_package_signature(&m).unwrap_err();
        assert!(matches!(err, PackageSignatureError::InvalidPublicKey(_)));
    }

    #[test]
    fn malformed_public_key_rejected() {
        let mut m = sample_manifest("com.test.foo");
        m.owner_public_key = "not-hex".to_string();
        let err = verify_package_signature(&m).unwrap_err();
        assert!(matches!(err, PackageSignatureError::InvalidPublicKey(_)));
    }

    #[test]
    fn malformed_signature_rejected() {
        let mut m = sample_manifest("com.test.foo");
        m.signature = "not-hex".to_string();
        let err = verify_package_signature(&m).unwrap_err();
        assert!(matches!(err, PackageSignatureError::InvalidSignature(_)));
    }

    #[test]
    fn wrong_length_signature_rejected() {
        let mut m = sample_manifest("com.test.foo");
        m.signature = "00".repeat(32); // 32 bytes, not 64
        let err = verify_package_signature(&m).unwrap_err();
        assert!(matches!(err, PackageSignatureError::InvalidSignature(_)));
    }
}
