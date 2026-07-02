//! طبقة أمان NXP.
//!
//! انظر RFC §5. توفر:
//! - AEAD (ChaCha20-Poly1305) لسرية + نزاهة الإطار
//! - توقيع / تحقق Ed25519 للإطارات المميزة
//! - X25519 ECDHE لاتفاق مفتاح الجلسة
//! - HKDF-SHA256 لاشتقاق المفاتيح
//! - نافذة منع إعادة التشغيل للحماية القائمة على nonce

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
