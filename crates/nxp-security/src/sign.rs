//! Frame signing and verification (Ed25519).
//!
//! See RFC §5.1. Ed25519 signatures are appended to a frame when
//! `FrameFlags::SIGNED` is set. The signed message is the concatenation
//! of the encoded frame header, the ciphertext payload, and the AEAD auth
//! tag. This binds the signature to exactly the bytes that will be
//! transmitted.

use ed25519_dalek::{Signature, VerifyingKey};
use nxp_core::NxpError;
use nxp_core::error::protocol_codes;

/// Signing/verification error.
#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    /// Signature did not verify against the message and public key.
    #[error("signature verification failed")]
    Invalid,
    /// Public key bytes were invalid.
    #[error("invalid public key")]
    InvalidPublicKey,
    /// Signature bytes were invalid (wrong length).
    #[error("invalid signature bytes")]
    InvalidSignatureBytes,
}

impl From<SignatureError> for NxpError {
    fn from(e: SignatureError) -> Self {
        match e {
            SignatureError::Invalid => {
                NxpError::protocol(protocol_codes::SIGNATURE_FAILED, "ed25519 verify failed")
            }
            SignatureError::InvalidPublicKey => {
                NxpError::protocol(protocol_codes::SIGNATURE_FAILED, "invalid public key")
            }
            SignatureError::InvalidSignatureBytes => {
                NxpError::protocol(protocol_codes::SIGNATURE_FAILED, "invalid signature bytes")
            }
        }
    }
}

/// Signer abstraction. Allows swapping in HSM-backed signers in the future.
pub trait Signer: Send + Sync {
    /// Sign `msg` and return a 64-byte Ed25519 signature.
    fn sign(&self, msg: &[u8]) -> [u8; 64];
}

/// Verifier abstraction.
pub trait Verifier: Send + Sync {
    /// Verify a 64-byte Ed25519 signature against `msg`.
    fn verify(&self, msg: &[u8], signature: &[u8; 64]) -> Result<(), SignatureError>;
}

/// Default in-process signer backed by an `IdentityKey`.
/// from `nxp-security::keys`.
pub struct IdentitySigner {
    signing_key: ed25519_dalek::SigningKey,
}

impl IdentitySigner {
    /// Construct from a 32-byte seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            signing_key: ed25519_dalek::SigningKey::from_bytes(seed),
        }
    }

    /// Public verifying key.
    pub fn public_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }
}

impl Signer for IdentitySigner {
    fn sign(&self, msg: &[u8]) -> [u8; 64] {
        use ed25519_dalek::Signer as _;
        self.signing_key.sign(msg).to_bytes()
    }
}

/// Stateless verifier backed by a public key.
pub struct IdentityVerifier {
    public: VerifyingKey,
}

impl IdentityVerifier {
    /// Construct from 32-byte public key.
    pub fn from_public(pk: &[u8; 32]) -> Result<Self, SignatureError> {
        let public = VerifyingKey::from_bytes(pk).map_err(|_| SignatureError::InvalidPublicKey)?;
        Ok(Self { public })
    }
}

impl Verifier for IdentityVerifier {
    fn verify(&self, msg: &[u8], signature: &[u8; 64]) -> Result<(), SignatureError> {
        use ed25519_dalek::Verifier as _;
        let sig = Signature::from_bytes(signature);
        self.public
            .verify(msg, &sig)
            .map_err(|_| SignatureError::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn sign_and_verify() {
        let signing = SigningKey::generate(&mut OsRng);
        let public = signing.verifying_key();
        let signer = IdentitySigner {
            signing_key: signing,
        };
        let verifier = IdentityVerifier { public };
        let msg = b"hello nexora";
        let sig = signer.sign(msg);
        verifier.verify(msg, &sig).unwrap();
    }

    #[test]
    fn rejects_tampered_message() {
        let signing = SigningKey::generate(&mut OsRng);
        let public = signing.verifying_key();
        let signer = IdentitySigner {
            signing_key: signing,
        };
        let verifier = IdentityVerifier { public };
        let sig = signer.sign(b"original");
        let err = verifier.verify(b"tampered", &sig).unwrap_err();
        assert!(matches!(err, SignatureError::Invalid));
    }
}
