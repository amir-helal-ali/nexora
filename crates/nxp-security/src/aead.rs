//! AEAD (ChaCha20-Poly1305) for frame payloads.
//!
//! See RFC §5.1. Each frame's payload is encrypted with a per-frame nonce
//! and authenticated with the frame header as additional data (AAD). This
//! binds the ciphertext to its routing metadata, preventing opcode
//! substitution attacks.

use crate::replay::ReplayWindow;
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Key, Nonce,
};
use nxp_core::{FrameFlags, NxpError, Opcode, error::protocol_codes};
use zeroize::ZeroizeOnDrop;

/// AEAD operation error.
#[derive(Debug, thiserror::Error)]
pub enum AeadError {
    /// Encryption failed (rare; usually a key/nonce misuse).
    #[error("aead encrypt: {0}")]
    Encrypt(String),
    /// Decryption failed — auth tag did not verify.
    #[error("aead decrypt: auth tag verification failed")]
    DecryptFailed,
    /// Decryption failed — nonce was already seen.
    #[error("aead decrypt: replay detected")]
    ReplayDetected,
}

impl From<AeadError> for NxpError {
    fn from(e: AeadError) -> Self {
        match e {
            AeadError::Encrypt(msg) => NxpError::protocol(protocol_codes::ENCODE_FAILED, msg),
            AeadError::DecryptFailed => {
                NxpError::protocol(protocol_codes::AUTH_TAG_FAILED, "auth tag mismatch")
            }
            AeadError::ReplayDetected => {
                NxpError::protocol(protocol_codes::REPLAY_DETECTED, "nonce reused")
            }
        }
    }
}

/// 32-byte symmetric AEAD key.
pub type AeadKey = [u8; 32];

/// Frame-level AEAD context. Holds the symmetric key for one direction
/// (send or receive) of a session. The key is zeroized on drop.
#[derive(ZeroizeOnDrop)]
pub struct FrameAead {
    /// Raw key bytes — zeroized on drop.
    key: AeadKey,
    /// Cipher constructed from `key`. We rebuild it lazily if needed;
    /// here we just hold the cipher for performance.
    #[zeroize(skip)]
    cipher: ChaCha20Poly1305,
    /// Replay window for the receive direction. Unused on the send side.
    /// Boxed so that cloning a sender context does not duplicate window state.
    #[zeroize(skip)]
    replay: Option<Box<ReplayWindow>>,
}

impl std::fmt::Debug for FrameAead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameAead")
            .field("has_replay_window", &self.replay.is_some())
            .finish_non_exhaustive()
    }
}

impl FrameAead {
    /// Construct a sender AEAD context (no replay window).
    pub fn new_sender(key: &AeadKey) -> Self {
        Self {
            key: *key,
            cipher: ChaCha20Poly1305::new(Key::from_slice(key)),
            replay: None,
        }
    }

    /// Construct a receiver AEAD context with a fresh replay window.
    pub fn new_receiver(key: &AeadKey) -> Self {
        Self {
            key: *key,
            cipher: ChaCha20Poly1305::new(Key::from_slice(key)),
            replay: Some(Box::new(ReplayWindow::new())),
        }
    }

    /// Encrypt a plaintext payload for a frame.
    ///
    /// The AAD is constructed from the immutable header fields:
    /// `(magic, version, flags, opcode, stream_id, request_id, timestamp_us)`.
    /// The nonce and payload_len are NOT included in AAD because they are
    /// derived/verified separately.
    pub fn encrypt(
        &self,
        nonce: &[u8; 12],
        aad: &Aad,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, AeadError> {
        let nonce = Nonce::from_slice(nonce);
        let payload = Payload {
            msg: plaintext,
            aad: aad.as_bytes(),
        };
        self.cipher
            .encrypt(nonce, payload)
            .map_err(|e| AeadError::Encrypt(e.to_string()))
    }

    /// Decrypt a ciphertext payload. On the receive side, also enforces
    /// replay protection via the embedded nonce window.
    pub fn decrypt(
        &mut self,
        nonce: &[u8; 12],
        aad: &Aad,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, AeadError> {
        // Replay check (receive side only).
        if let Some(window) = self.replay.as_mut() {
            if !window.check_and_insert(nonce) {
                return Err(AeadError::ReplayDetected);
            }
        }
        let nonce = Nonce::from_slice(nonce);
        let payload = Payload {
            msg: ciphertext,
            aad: aad.as_bytes(),
        };
        self.cipher
            .decrypt(nonce, payload)
            .map_err(|_| AeadError::DecryptFailed)
    }
}

/// Additional authenticated data for a frame. Constructed from the immutable
/// header fields that must be bound to the ciphertext.
///
/// Layout: `magic(2) + version(1) + flags(2) + opcode(2) + stream_id(4)
/// + request_id(8) + timestamp_us(8) = 27 bytes`.
#[derive(Clone, Copy)]
pub struct Aad {
    buf: [u8; 27],
}

impl Aad {
    /// Build AAD from frame header fields.
    pub fn new(
        version: u8,
        flags: FrameFlags,
        opcode: Opcode,
        stream_id: u32,
        request_id: u64,
        timestamp_us: u64,
    ) -> Self {
        let mut buf = [0u8; 27];
        buf[0..2].copy_from_slice(&nxp_core::MAGIC);
        buf[2] = version;
        buf[3..5].copy_from_slice(&flags.bits().to_be_bytes());
        buf[5..7].copy_from_slice(&opcode.as_u16().to_be_bytes());
        buf[7..11].copy_from_slice(&stream_id.to_be_bytes());
        buf[11..19].copy_from_slice(&request_id.to_be_bytes());
        buf[19..27].copy_from_slice(&timestamp_us.to_be_bytes());
        Self { buf }
    }

    /// Serialize to bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_key() -> AeadKey {
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = i as u8;
        }
        k
    }

    fn sample_aad() -> Aad {
        Aad::new(
            1,
            FrameFlags::ENCRYPTED,
            Opcode::Ping,
            7,
            42,
            1_700_000_000_000_000,
        )
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let sender = FrameAead::new_sender(&sample_key());
        let mut receiver = FrameAead::new_receiver(&sample_key());
        let nonce = [0xAA; 12];
        let aad = sample_aad();
        let plaintext = b"hello nexora";
        let ct = sender.encrypt(&nonce, &aad, plaintext).unwrap();
        let pt = receiver.decrypt(&nonce, &aad, &ct).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn decrypt_rejects_tampered_aad() {
        let sender = FrameAead::new_sender(&sample_key());
        let mut receiver = FrameAead::new_receiver(&sample_key());
        let nonce = [0xAA; 12];
        let aad = sample_aad();
        let ct = sender.encrypt(&nonce, &aad, b"hello").unwrap();

        // Tamper with AAD: change opcode.
        let mut bad_aad = aad;
        bad_aad.buf[5..7].copy_from_slice(&Opcode::Pong.as_u16().to_be_bytes());

        let err = receiver.decrypt(&nonce, &bad_aad, &ct).unwrap_err();
        assert!(matches!(err, AeadError::DecryptFailed));
    }

    #[test]
    fn decrypt_rejects_replay() {
        let sender = FrameAead::new_sender(&sample_key());
        let mut receiver = FrameAead::new_receiver(&sample_key());
        let nonce = [0xAA; 12];
        let aad = sample_aad();
        let ct = sender.encrypt(&nonce, &aad, b"hello").unwrap();

        // First decrypt succeeds.
        let pt = receiver.decrypt(&nonce, &aad, &ct).unwrap();
        assert_eq!(pt, b"hello");

        // Replay with same nonce fails.
        let err = receiver.decrypt(&nonce, &aad, &ct).unwrap_err();
        assert!(matches!(err, AeadError::ReplayDetected));
    }

    #[test]
    fn different_keys_fail_to_decrypt() {
        let k1 = [1u8; 32];
        let k2 = [2u8; 32];
        let sender = FrameAead::new_sender(&k1);
        let mut receiver = FrameAead::new_receiver(&k2);
        let nonce = [0u8; 12];
        let aad = sample_aad();
        let ct = sender.encrypt(&nonce, &aad, b"hello").unwrap();
        let err = receiver.decrypt(&nonce, &aad, &ct).unwrap_err();
        assert!(matches!(err, AeadError::DecryptFailed));
    }
}
