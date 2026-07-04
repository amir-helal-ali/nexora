//! NXP — Nexora Exchange Protocol
//!
//! Core protocol primitives: frames, opcodes, flags, errors.
//! See `docs/NXP-RFC-v1.md` for the full specification.

#![forbid(unsafe_code)]
#![deny(rust_2021_compatibility)]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms)]

pub mod error;
pub mod flags;
pub mod frame;
pub mod opcode;
pub mod version;

pub use error::{NxpError, ErrorScope, Result};
pub use flags::FrameFlags;
pub use frame::{Frame, FrameHeader, AUTH_TAG_LEN, HEADER_LEN, MAGIC, MAX_PAYLOAD_LEN, NONCE_LEN, SIGNATURE_LEN};
pub use opcode::Opcode;
pub use version::VERSION;
