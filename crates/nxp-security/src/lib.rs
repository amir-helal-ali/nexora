//! NXP security layer.
//!
//! See RFC §5. Provides:
//! - AEAD (ChaCha20-Poly1305) for frame confidentiality + integrity
//! - Ed25519 signing / verification for privileged frames
//! - X25519 ECDHE for session key agreement
//! - HKDF-SHA256 for key derivation
//! - Replay window for nonce-based replay protection

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod aead;
pub mod keys;
pub mod replay;
pub mod sign;

pub use aead::{AeadError, AeadKey, FrameAead};
pub use keys::{IdentityKey, SessionId, SessionKeys, SessionSecret};
pub use replay::{ReplayError, ReplayWindow};
pub use sign::{SignatureError, Signer, Verifier};
