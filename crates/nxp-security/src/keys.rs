//! Session key derivation and identity keys.
//!
//! See RFC §5.3, §5.5. NXP uses X25519 ECDHE to derive a shared secret at
//! session setup, then HKDF-SHA256 to expand it into per-direction AEAD
//! keys plus a session ID. Identity keys are Ed25519 keypairs for signing
//! privileged frames.

use crate::aead::AeadKey;
use ed25519_dalek::{SigningKey, VerifyingKey};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::ZeroizeOnDrop;

/// 16-byte session identifier.
pub type SessionId = [u8; 16];

/// All derived session material. All fields are zeroized on drop.
#[derive(ZeroizeOnDrop)]
pub struct SessionKeys {
    /// Session identifier (sent in `HELLO_ACK`).
    pub session_id: SessionId,
    /// AEAD key for client → server frames.
    pub client_to_server: AeadKey,
    /// AEAD key for server → client frames.
    pub server_to_client: AeadKey,
}

impl std::fmt::Debug for SessionKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionKeys")
            .field("session_id", &hex::encode(self.session_id))
            .finish_non_exhaustive()
    }
}

/// Ephemeral X25519 secret + public key used in the `HELLO` handshake.
/// The secret is zeroized on drop.
#[derive(ZeroizeOnDrop)]
pub struct SessionSecret {
    secret: StaticSecret,
    public: X25519PublicKey,
}

impl SessionSecret {
    /// Generate a new ephemeral keypair.
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(&mut OsRng);
        let public = X25519PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Public key to send to the peer.
    pub fn public_key(&self) -> X25519PublicKey {
        self.public
    }

    /// Derive session keys from the peer's public key. Consumes the secret.
    pub fn derive(self, peer_public: &X25519PublicKey) -> SessionKeys {
        let shared = self.secret.diffie_hellman(peer_public);
        let hkdf = Hkdf::<Sha256>::new(None, shared.as_bytes());

        let mut session_id = [0u8; 16];
        hkdf.expand(b"nxp session id v1", &mut session_id)
            .expect("expand session_id");

        let mut c2s = [0u8; 32];
        let mut s2c = [0u8; 32];
        hkdf.expand(b"nxp c2s key v1", &mut c2s).expect("expand c2s");
        hkdf.expand(b"nxp s2c key v1", &mut s2c).expect("expand s2c");

        SessionKeys {
            session_id,
            client_to_server: c2s,
            server_to_client: s2c,
        }
    }
}

/// Ed25519 signing keypair. Used by services and plugins to sign frames
/// for non-repudiation. Private key is zeroized on drop.
#[derive(ZeroizeOnDrop)]
pub struct IdentityKey {
    signing: SigningKey,
}

impl IdentityKey {
    /// Generate a fresh identity keypair.
    pub fn generate() -> Self {
        Self {
            signing: SigningKey::generate(&mut OsRng),
        }
    }

    /// Construct from existing secret bytes (32 bytes).
    pub fn from_bytes(secret: &[u8; 32]) -> Self {
        Self {
            signing: SigningKey::from_bytes(secret),
        }
    }

    /// Public verifying key (32 bytes).
    pub fn public_key(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }

    /// Public key bytes.
    pub fn public_bytes(&self) -> [u8; 32] {
        self.public_key().to_bytes()
    }

    /// Sign a message. Returns a 64-byte signature.
    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        use ed25519_dalek::Signer;
        self.signing.sign(msg).to_bytes()
    }
}

impl std::fmt::Debug for IdentityKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentityKey")
            .field("public", &hex::encode(self.public_bytes()))
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecdhe_derives_symmetric_keys() {
        let client = SessionSecret::generate();
        let server = SessionSecret::generate();
        // Capture public keys BEFORE derive() consumes the secrets.
        let client_pub = client.public_key();
        let server_pub = server.public_key();
        let client_keys = client.derive(&server_pub);
        let server_keys = server.derive(&client_pub);
        // Both sides must derive the same keys.
        assert_eq!(client_keys.session_id, server_keys.session_id);
        assert_eq!(client_keys.client_to_server, server_keys.client_to_server);
        assert_eq!(client_keys.server_to_client, server_keys.server_to_client);
    }

    #[test]
    fn identity_key_signs_and_verifies() {
        let key = IdentityKey::generate();
        let msg = b"hello nexora";
        let sig = key.sign(msg);
        use ed25519_dalek::Verifier;
        let verifying = key.public_key();
        assert!(verifying
            .verify(msg, &ed25519_dalek::Signature::from_bytes(&sig))
            .is_ok());
        assert!(verifying
            .verify(b"tampered", &ed25519_dalek::Signature::from_bytes(&sig))
            .is_err());
    }

    #[test]
    fn session_keys_differ_per_session() {
        let a1 = SessionSecret::generate();
        let a2 = SessionSecret::generate();
        let b = SessionSecret::generate();
        let b_pub = b.public_key();
        let k1 = a1.derive(&b_pub);
        let k2 = a2.derive(&b_pub);
        assert_ne!(k1.session_id, k2.session_id);
    }
}
