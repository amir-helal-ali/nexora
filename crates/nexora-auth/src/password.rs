//! Password hashing (Argon2id).
//!
//! See Nexora Engineering Specification, Part 9 (AUTHENTICATION SYSTEM).
//! Passwords are hashed with Argon2id and a random 16-byte salt. The hash
//! is stored as a PHC string (`$argon2id$v=19$m=...`).
//!
//! Verifiers run in constant time. The hash parameters are deliberately
//! tuned for a low-resource Tier-1 VPS: m=19456 KiB, t=2, p=1.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use std::fmt;
use zeroize::Zeroize;

/// Password operation error.
#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    /// Hashing failed (rare; usually a misconfiguration).
    #[error("argon2 hash failed: {0}")]
    HashFailed(String),
    /// Verification failed (wrong password or corrupted hash).
    #[error("password verification failed")]
    VerifyFailed,
    /// Hash string was malformed.
    #[error("invalid password hash string: {0}")]
    InvalidHash(String),
}

/// A hashed password (PHC string). Zeroized on drop.
pub struct HashedPassword(String);

impl Zeroize for HashedPassword {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl Drop for HashedPassword {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl fmt::Debug for HashedPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HashedPassword(***)")
    }
}

impl HashedPassword {
    /// Returns the PHC string representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse from a PHC string.
    pub fn from_str(phc: &str) -> Result<Self, PasswordError> {
        // Validate by attempting to parse.
        PasswordHash::new(phc).map_err(|e| PasswordError::InvalidHash(e.to_string()))?;
        Ok(Self(phc.to_string()))
    }
}

impl Clone for HashedPassword {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Hash a password using Argon2id with a random salt.
pub fn hash_password(password: &str) -> Result<HashedPassword, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| PasswordError::HashFailed(e.to_string()))?
        .to_string();
    Ok(HashedPassword(hash))
}

/// Verify a password against a stored hash. Runs in constant time.
///
/// Returns `Ok(())` if the password matches, `Err(VerifyFailed)` otherwise.
pub fn verify_password(password: &str, stored: &HashedPassword) -> Result<(), PasswordError> {
    let parsed = PasswordHash::new(stored.as_str())
        .map_err(|e| PasswordError::InvalidHash(e.to_string()))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| PasswordError::VerifyFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let hashed = hash_password("hunter2").unwrap();
        assert!(verify_password("hunter2", &hashed).is_ok());
        assert!(verify_password("wrong", &hashed).is_err());
    }

    #[test]
    fn hash_is_unique_per_call() {
        let h1 = hash_password("same").unwrap();
        let h2 = hash_password("same").unwrap();
        assert_ne!(h1.as_str(), h2.as_str()); // different salts
        assert!(verify_password("same", &h1).is_ok());
        assert!(verify_password("same", &h2).is_ok());
    }

    #[test]
    fn invalid_hash_rejected() {
        let err = HashedPassword::from_str("not-a-real-hash").unwrap_err();
        assert!(matches!(err, PasswordError::InvalidHash(_)));
    }

    #[test]
    fn debug_does_not_leak_hash() {
        let hashed = hash_password("secret").unwrap();
        let s = format!("{:?}", hashed);
        assert!(!s.contains("argon2"));
        assert!(s.contains("***"));
    }
}
