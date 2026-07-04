//! NXP error model.
//!
//! See RFC §8. Errors are carried in `ERROR` frames and structured for
//! machine consumption, not for direct user display.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Top-level NXP `Result` alias.
pub type Result<T> = std::result::Result<T, NxpError>;

/// Error scope. Determines the namespace of `code`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ErrorScope {
    /// 0x0000–0x00FF — Protocol-level errors (malformed frames, bad magic, version mismatch).
    Protocol = 0,
    /// 0x0100–0x01FF — Session errors (expired session, replay, heartbeat timeout).
    Session = 1,
    /// 0x0200–0x02FF — Authentication errors.
    Auth = 2,
    /// 0x0300–0x03FF — Authorization errors.
    Authz = 3,
    /// 0x1000–0xFFFF — Application-defined errors.
    App = 4,
    /// 0xFF00–0xFFFF — Catch-all internal errors (always non-retryable).
    Internal = 5,
}

/// Stable error codes within the [`ErrorScope::Protocol`] namespace.
pub mod protocol_codes {
    /// Frame magic bytes did not match `0x4E58`.
    pub const BAD_MAGIC: u32 = 0x0001;
    /// Unsupported wire-format version.
    pub const BAD_VERSION: u32 = 0x0002;
    /// Frame too short to contain a header.
    pub const TRUNCATED_HEADER: u32 = 0x0003;
    /// Payload length exceeds `MAX_PAYLOAD_LEN`.
    pub const PAYLOAD_TOO_LARGE: u32 = 0x0004;
    /// AEAD authentication tag did not verify.
    pub const AUTH_TAG_FAILED: u32 = 0x0005;
    /// Ed25519 signature did not verify.
    pub const SIGNATURE_FAILED: u32 = 0x0006;
    /// Timestamp skew exceeds tolerance (±60s).
    pub const TIMESTAMP_SKEW: u32 = 0x0007;
    /// Nonce was reused within the session's replay window.
    pub const REPLAY_DETECTED: u32 = 0x0008;
    /// Unknown opcode and not in the application namespace.
    pub const UNKNOWN_OPCODE: u32 = 0x0009;
    /// Frame failed to deserialize as expected by the handler.
    pub const DECODE_FAILED: u32 = 0x000A;
    /// Frame failed to encode.
    pub const ENCODE_FAILED: u32 = 0x000B;
}

/// Stable error codes within the [`ErrorScope::Session`] namespace.
pub mod session_codes {
    /// Session ID unknown or expired.
    pub const UNKNOWN_SESSION: u32 = 0x0100;
    /// Heartbeat missed for too long.
    pub const HEARTBEAT_TIMEOUT: u32 = 0x0101;
    /// Session resumption token was invalid.
    pub const BAD_RESUME_TOKEN: u32 = 0x0102;
}

/// Stable error codes within the [`ErrorScope::Auth`] namespace.
pub mod auth_codes {
    /// No credentials provided.
    pub const NO_CREDENTIALS: u32 = 0x0200;
    /// Credentials provided but invalid.
    pub const INVALID_CREDENTIALS: u32 = 0x0201;
    /// Token expired.
    pub const TOKEN_EXPIRED: u32 = 0x0202;
}

/// Stable error codes within the [`ErrorScope::Authz`] namespace.
pub mod authz_codes {
    /// Identity is not permitted to invoke this opcode on this resource.
    pub const FORBIDDEN: u32 = 0x0300;
    /// Identity lacks a required capability.
    pub const MISSING_CAPABILITY: u32 = 0x0301;
}

/// NXP wire-level error. Carried inside an `ERROR` frame.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NxpError {
    /// Stable, namespace-scoped error code.
    pub code: u32,
    /// Scope of the error.
    pub scope: ErrorScope,
    /// Human-readable message (English, for logs only — never shown to end users).
    pub message: String,
    /// Whether the caller should retry.
    pub retryable: bool,
    /// Arbitrary structured details.
    #[serde(default)]
    pub details: serde_value::Value,
}

// We do NOT pull `serde_value` into the workspace by default; alias it to a
// minimal locally-defined enum so the wire format stays MessagePack-friendly
// and we don't grow a new dependency. The public API stays stable.
mod serde_value {
    use serde::{Deserialize, Serialize};
    /// Minimal dynamic value used for `NxpError::details`.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum Value {
        /// Null.
        Null,
        /// Boolean.
        Bool(bool),
        /// 64-bit signed integer.
        Int(i64),
        /// 64-bit unsigned integer.
        UInt(u64),
        /// 64-bit float.
        Float(f64),
        /// UTF-8 string.
        Str(String),
        /// Binary blob.
        Bytes(Vec<u8>),
        /// Array of values.
        Array(Vec<Value>),
        /// Map of string → value.
        Map(Vec<(String, Value)>),
    }
    impl Default for Value {
        fn default() -> Self {
            Self::Null
        }
    }
}

pub use serde_value::Value;

impl NxpError {
    /// Construct a new error.
    pub fn new(scope: ErrorScope, code: u32, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            scope,
            code,
            message: message.into(),
            retryable,
            details: Value::Null,
        }
    }

    /// Attach structured details.
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    /// Shortcut: protocol-level error.
    pub fn protocol(code: u32, message: impl Into<String>) -> Self {
        Self::new(ErrorScope::Protocol, code, message, false)
    }

    /// Shortcut: session-level error.
    pub fn session(code: u32, message: impl Into<String>, retryable: bool) -> Self {
        Self::new(ErrorScope::Session, code, message, retryable)
    }

    /// Shortcut: auth-level error.
    pub fn auth(code: u32, message: impl Into<String>) -> Self {
        Self::new(ErrorScope::Auth, code, message, false)
    }

    /// Shortcut: authz-level error.
    pub fn authz(code: u32, message: impl Into<String>) -> Self {
        Self::new(ErrorScope::Authz, code, message, false)
    }

    /// Shortcut: internal error (always non-retryable).
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorScope::Internal, 0xFF00, message, false)
    }

    /// Encode to MessagePack bytes for inclusion in an ERROR frame payload.
    pub fn encode_msgpack(&self) -> std::result::Result<Vec<u8>, rmp_serde::encode::Error> {
        rmp_serde::to_vec_named(self)
    }

    /// Decode from MessagePack bytes.
    pub fn decode_msgpack(bytes: &[u8]) -> std::result::Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(bytes)
    }
}

impl fmt::Display for NxpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scope_name = match self.scope {
            ErrorScope::Protocol => "PROTOCOL",
            ErrorScope::Session => "SESSION",
            ErrorScope::Auth => "AUTH",
            ErrorScope::Authz => "AUTHZ",
            ErrorScope::App => "APP",
            ErrorScope::Internal => "INTERNAL",
        };
        write!(
            f,
            "NxpError {{ scope={}, code=0x{:04X}, retryable={}, msg=\"{}\" }}",
            scope_name, self.code, self.retryable, self.message
        )
    }
}

impl std::error::Error for NxpError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construct_and_display() {
        let e = NxpError::protocol(protocol_codes::BAD_MAGIC, "bad magic bytes");
        assert_eq!(e.scope, ErrorScope::Protocol);
        assert!(!e.retryable);
        let s = format!("{}", e);
        assert!(s.contains("PROTOCOL"));
        assert!(s.contains("0x0001"));
    }

    #[test]
    fn msgpack_roundtrip() {
        let e = NxpError::session(session_codes::HEARTBEAT_TIMEOUT, "missed 3 heartbeats", true)
            .with_details(Value::Array(vec![
                Value::UInt(1),
                Value::UInt(2),
                Value::UInt(3),
            ]));
        let bytes = e.encode_msgpack().unwrap();
        let e2 = NxpError::decode_msgpack(&bytes).unwrap();
        assert_eq!(e.scope, e2.scope);
        assert_eq!(e.code, e2.code);
        assert_eq!(e.message, e2.message);
        assert_eq!(e.retryable, e2.retryable);
    }
}
