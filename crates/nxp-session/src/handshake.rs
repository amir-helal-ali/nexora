//! NXP `HELLO` / `HELLO_ACK` handshake.
//!
//! See RFC §2.2. The handshake establishes a session by exchanging X25519
//! public keys, deriving shared keys via HKDF, and negotiating capabilities.

use crate::time::now_us;
use nxp_security::{IdentityKey, SessionId, SessionKeys, SessionSecret};
use serde::{Deserialize, Serialize};

/// Handshake error.
#[derive(Debug, thiserror::Error)]
pub enum HandshakeError {
    /// Client and server have no overlapping capabilities.
    #[error("no overlapping capabilities")]
    NoOverlappingCapabilities,
    /// Server rejected the client's identity.
    #[error("client identity rejected")]
    IdentityRejected,
}

/// `HELLO` payload sent by the client to initiate a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloPayload {
    /// Client's ephemeral X25519 public key (32 bytes).
    #[serde(with = "serde_bytes")]
    pub client_public: Vec<u8>,
    /// Client's long-term Ed25519 public key (32 bytes), if authenticated.
    #[serde(with = "serde_bytes")]
    pub client_identity: Vec<u8>,
    /// Client-supported NXP version.
    pub version: u8,
    /// Client-supported capabilities (bitmask). Reserved for future use.
    pub capabilities: u32,
    /// Optional opaque auth token (e.g. OAuth2 access token).
    #[serde(with = "serde_bytes")]
    pub auth_token: Vec<u8>,
    /// Wall-clock timestamp (microseconds) from the client.
    pub timestamp_us: u64,
}

/// `HELLO_ACK` payload sent by the server to accept a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloAckPayload {
    /// Server's ephemeral X25519 public key (32 bytes).
    #[serde(with = "serde_bytes")]
    pub server_public: Vec<u8>,
    /// Negotiated session ID (16 bytes).
    #[serde(with = "serde_bytes")]
    pub session_id: Vec<u8>,
    /// Negotiated NXP version.
    pub version: u8,
    /// Negotiated capabilities (intersection).
    pub capabilities: u32,
    /// Session expiry timestamp (microseconds).
    pub expires_at_us: u64,
}

/// Client-side handshake state. Holds the ephemeral secret until the
/// server's `HELLO_ACK` arrives.
pub struct ClientHandshake {
    secret: SessionSecret,
    identity: Option<IdentityKey>,
    capabilities: u32,
}

impl std::fmt::Debug for ClientHandshake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientHandshake")
            .field("has_identity", &self.identity.is_some())
            .field("capabilities", &self.capabilities)
            .finish_non_exhaustive()
    }
}

impl ClientHandshake {
    /// Begin a new client handshake. Optionally attach a long-term identity
    /// key for authenticated sessions.
    pub fn new(identity: Option<IdentityKey>, capabilities: u32) -> Self {
        Self {
            secret: SessionSecret::generate(),
            identity,
            capabilities,
        }
    }

    /// Build the `HELLO` payload to send to the server.
    pub fn hello_payload(&self, auth_token: Vec<u8>) -> HelloPayload {
        HelloPayload {
            client_public: self.secret.public_key().as_bytes().to_vec(),
            client_identity: self
                .identity
                .as_ref()
                .map(|k| k.public_bytes().to_vec())
                .unwrap_or_default(),
            version: nxp_core::VERSION,
            capabilities: self.capabilities,
            auth_token,
            timestamp_us: now_us(),
        }
    }

    /// Consume the handshake and derive session keys from the server's
    /// `HELLO_ACK`.
    pub fn finalize(self, ack: &HelloAckPayload) -> Result<SessionKeys, HandshakeError> {
        if ack.server_public.len() != 32 {
            return Err(HandshakeError::IdentityRejected);
        }
        let mut pk_bytes = [0u8; 32];
        pk_bytes.copy_from_slice(&ack.server_public);
        let server_public = x25519_dalek::PublicKey::from(pk_bytes);
        let keys = self.secret.derive(&server_public);

        // Sanity check: the server should echo back our derived session ID.
        if ack.session_id.len() != 16 {
            return Err(HandshakeError::IdentityRejected);
        }
        let mut echoed = SessionId::default();
        echoed.copy_from_slice(&ack.session_id);
        if echoed != keys.session_id {
            return Err(HandshakeError::IdentityRejected);
        }
        Ok(keys)
    }
}

/// Server-side handshake state.
pub struct ServerHandshake {
    secret: SessionSecret,
    capabilities: u32,
}

impl ServerHandshake {
    /// Begin a new server handshake.
    pub fn new(capabilities: u32) -> Self {
        Self {
            secret: SessionSecret::generate(),
            capabilities,
        }
    }

    /// Process a client `HELLO` and produce the `HELLO_ACK` plus derived
    /// session keys. The server should store the keys for the new session.
    pub fn accept(
        self,
        hello: &HelloPayload,
    ) -> Result<(HelloAckPayload, SessionKeys), HandshakeError> {
        if hello.client_public.len() != 32 {
            return Err(HandshakeError::IdentityRejected);
        }
        let mut pk_bytes = [0u8; 32];
        pk_bytes.copy_from_slice(&hello.client_public);
        let client_public = x25519_dalek::PublicKey::from(pk_bytes);

        // Snapshot the server's public key BEFORE derive() consumes self.secret.
        let server_public_bytes = self.secret.public_key().as_bytes().to_vec();
        let keys = self.secret.derive(&client_public);

        // Capability negotiation: intersection.
        let negotiated = self.capabilities & hello.capabilities;

        let ack = HelloAckPayload {
            server_public: server_public_bytes,
            session_id: keys.session_id.to_vec(),
            version: nxp_core::VERSION,
            capabilities: negotiated,
            expires_at_us: now_us() + 3_600 * 1_000_000, // 1h
        };
        Ok((ack, keys))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_server_handshake_roundtrip() {
        let client = ClientHandshake::new(None, 0xFFFF_FFFF);
        let server = ServerHandshake::new(0xFFFF_FFFF);
        let hello = client.hello_payload(vec![]);
        let (ack, server_keys) = server.accept(&hello).unwrap();
        // hello_payload borrows, so `client` is still owned and can be finalized.
        let client_keys = client.finalize(&ack).unwrap();
        // Both sides must derive identical keys.
        assert_eq!(client_keys.session_id, server_keys.session_id);
        assert_eq!(client_keys.client_to_server, server_keys.client_to_server);
        assert_eq!(client_keys.server_to_client, server_keys.server_to_client);
    }
}
