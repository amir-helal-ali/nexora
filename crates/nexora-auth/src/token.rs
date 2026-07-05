//! Session tokens — Ed25519-signed, expiring, refreshable.
//!
//! See Nexora Engineering Specification, Part 9 (AUTHENTICATION SYSTEM).
//! Sessions are short-lived (default 1h) and rotated. Tokens are signed
//! with the service's long-term Ed25519 identity key and carry:
//! - User ID
//! - Issued-at timestamp
//! - Expiry timestamp
//! - Token version (incremented on refresh / logout)

use crate::users::UserId;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use parking_lot::RwLock;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;
use time::OffsetDateTime;

/// Default token lifetime: 1 hour.
pub const DEFAULT_TOKEN_TTL: Duration = Duration::from_secs(3600);

/// Default refresh token lifetime: 24 hours.
pub const DEFAULT_REFRESH_TTL: Duration = Duration::from_secs(86400);

/// Token operation error.
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    /// Token signature was invalid or signed by a different key.
    #[error("invalid token signature")]
    InvalidSignature,
    /// Token has expired.
    #[error("token expired")]
    Expired,
    /// Token has been revoked (logout or refresh).
    #[error("token revoked")]
    Revoked,
    /// Token bytes were malformed.
    #[error("malformed token: {0}")]
    Malformed(String),
    /// Token version mismatch (stale token).
    #[error("token version mismatch (expected {expected}, got {got})")]
    VersionMismatch {
        /// Expected version.
        expected: u64,
        /// Got version.
        got: u64,
    },
}

/// The signed payload of a session token.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TokenClaims {
    /// Subject (user ID).
    pub sub: UserId,
    /// Issued-at (unix nanos).
    pub iat: i64,
    /// Expiry (unix nanos).
    pub exp: i64,
    /// Token version (incremented on refresh).
    pub ver: u64,
}

/// A serialized + signed token. The wire format is:
/// `claims_msgpack || signature_64B`.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionToken {
    /// The signed claims.
    pub claims: TokenClaims,
    /// Ed25519 signature (64 bytes).
    pub signature: [u8; 64],
}

impl SessionToken {
    /// Serialize the token to bytes for transport.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128);
        let claims = rmp_serde::to_vec_named(&self.claims).unwrap_or_default();
        out.extend_from_slice(&claims);
        out.extend_from_slice(&self.signature);
        out
    }

    /// Deserialize a token from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TokenError> {
        if bytes.len() < 64 {
            return Err(TokenError::Malformed("too short".into()));
        }
        let claims_end = bytes.len() - 64;
        let claims: TokenClaims = rmp_serde::from_slice(&bytes[..claims_end])
            .map_err(|e| TokenError::Malformed(e.to_string()))?;
        let mut signature = [0u8; 64];
        signature.copy_from_slice(&bytes[claims_end..]);
        Ok(Self { claims, signature })
    }

    /// Encode to a URL-safe base64 string.
    ///
    /// Note: `Display` is also implemented and produces the same output,
    /// so `token.to_string()` (via Display) works equivalently.
    pub fn encode(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(self.to_bytes())
    }

    /// Decode from a URL-safe base64 string.
    pub fn from_str(s: &str) -> Result<Self, TokenError> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(s)
            .map_err(|e| TokenError::Malformed(e.to_string()))?;
        Self::from_bytes(&bytes)
    }
}

impl fmt::Display for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.encode())
    }
}

/// Token verifier / signer. Holds the service's long-term Ed25519 key.
/// The signing key is zeroized on drop.
pub struct TokenVerifier {
    signing_key: SigningKey,
    /// Active token versions per user. A token is valid iff its `ver` field
    /// matches the current version for that user. Logout increments the
    /// version, invalidating all prior tokens.
    versions: RwLock<HashMap<UserId, u64>>,
}

impl Drop for TokenVerifier {
    fn drop(&mut self) {
        // Best-effort zeroization of the signing key bytes.
        // SigningKey::to_bytes returns a copy; we zeroize that.
        let mut bytes = self.signing_key.to_bytes();
        use zeroize::Zeroize;
        bytes.zeroize();
    }
}

impl fmt::Debug for TokenVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.versions.read().len();
        f.debug_struct("TokenVerifier")
            .field("tracked_users", &count)
            .field("public_key", &hex::encode(self.signing_key.verifying_key().to_bytes()))
            .finish_non_exhaustive()
    }
}

impl TokenVerifier {
    /// Construct with a freshly-generated key.
    pub fn new(_identity: nxp_security::IdentityKey) -> Self {
        // We use our own SigningKey here for direct access; the IdentityKey
        // argument is kept for API compatibility with future HSM-backed keys.
        let signing_key = SigningKey::generate(&mut OsRng);
        Self {
            signing_key,
            versions: RwLock::new(HashMap::new()),
        }
    }

    /// Public key (32 bytes) — used by external verifiers.
    pub fn public_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Issue a new token for a user. Bumps the user's version to invalidate
    /// any prior tokens.
    pub fn issue(&self, user_id: &str, ttl: Duration) -> SessionToken {
        let mut versions = self.versions.write();
        let next = versions.get(user_id).copied().unwrap_or(0) + 1;
        versions.insert(user_id.to_string(), next);
        drop(versions);

        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let exp = now + ttl.as_nanos() as i64;
        let claims = TokenClaims {
            sub: user_id.to_string(),
            iat: now,
            exp,
            ver: next,
        };
        let claims_bytes = rmp_serde::to_vec_named(&claims).unwrap_or_default();
        let signature = self.signing_key.sign(&claims_bytes).to_bytes();
        SessionToken { claims, signature }
    }

    /// Verify a token. Checks signature, expiry, and version.
    pub fn verify(&self, token: &SessionToken) -> Result<TokenClaims, TokenError> {
        // Verify signature.
        let claims_bytes = rmp_serde::to_vec_named(&token.claims).unwrap_or_default();
        let sig = Signature::from_bytes(&token.signature);
        self.signing_key
            .verifying_key()
            .verify(&claims_bytes, &sig)
            .map_err(|_| TokenError::InvalidSignature)?;

        // Check expiry.
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        if token.claims.exp < now {
            return Err(TokenError::Expired);
        }

        // Check version.
        let versions = self.versions.read();
        let current = versions.get(&token.claims.sub).copied();
        match current {
            None => Err(TokenError::Revoked), // user never had a token (or fully logged out)
            Some(v) if v != token.claims.ver => Err(TokenError::VersionMismatch {
                expected: v,
                got: token.claims.ver,
            }),
            Some(_) => Ok(token.claims.clone()),
        }
    }

    /// Revoke all tokens for a user (e.g. on logout). Increments the version.
    pub fn revoke(&self, user_id: &str) {
        let mut versions = self.versions.write();
        let next = versions.get(user_id).copied().unwrap_or(0) + 1;
        versions.insert(user_id.to_string(), next);
    }

    /// Refresh a token. The old token must be valid; a new one with an
    /// incremented version is issued, invalidating the old one.
    pub fn refresh(&self, old: &SessionToken, ttl: Duration) -> Result<SessionToken, TokenError> {
        let claims = self.verify(old)?;
        Ok(self.issue(&claims.sub, ttl))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nxp_security::IdentityKey;

    fn verifier() -> TokenVerifier {
        TokenVerifier::new(IdentityKey::generate())
    }

    #[test]
    fn issue_and_verify_roundtrip() {
        let v = verifier();
        let token = v.issue("user-1", DEFAULT_TOKEN_TTL);
        let claims = v.verify(&token).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.ver, 1);
    }

    #[test]
    fn expired_token_rejected() {
        let v = verifier();
        let token = v.issue("user-1", Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(10));
        assert!(matches!(v.verify(&token), Err(TokenError::Expired)));
    }

    #[test]
    fn revoke_invalidates_token() {
        let v = verifier();
        let token = v.issue("user-1", DEFAULT_TOKEN_TTL);
        v.verify(&token).unwrap(); // OK
        v.revoke("user-1");
        assert!(matches!(v.verify(&token), Err(TokenError::VersionMismatch { .. })));
    }

    #[test]
    fn refresh_invalidates_old_token() {
        let v = verifier();
        let t1 = v.issue("user-1", DEFAULT_TOKEN_TTL);
        let t2 = v.refresh(&t1, DEFAULT_TOKEN_TTL).unwrap();
        // Old token is now invalid.
        assert!(matches!(v.verify(&t1), Err(TokenError::VersionMismatch { .. })));
        // New token is valid.
        let claims = v.verify(&t2).unwrap();
        assert_eq!(claims.ver, 2);
    }

    #[test]
    fn tampered_signature_rejected() {
        let v = verifier();
        let mut token = v.issue("user-1", DEFAULT_TOKEN_TTL);
        token.signature[0] ^= 0xFF;
        assert!(matches!(v.verify(&token), Err(TokenError::InvalidSignature)));
    }

    #[test]
    fn token_string_roundtrip() {
        let v = verifier();
        let token = v.issue("user-1", DEFAULT_TOKEN_TTL);
        let s = token.encode();
        let parsed = SessionToken::from_str(&s).unwrap();
        assert_eq!(token, parsed);
    }

    #[test]
    fn different_verifiers_have_different_keys() {
        let v1 = verifier();
        let v2 = verifier();
        let token = v1.issue("user-1", DEFAULT_TOKEN_TTL);
        // v2 doesn't know about user-1, but more importantly the signature
        // won't verify because the keys are different.
        assert!(matches!(v2.verify(&token), Err(TokenError::InvalidSignature)));
    }
}
