//! NXP session layer.
//!
//! See RFC §2.2. A session is established by a `HELLO` command immediately
//! after the QUIC handshake. This module provides the handshake protocol,
//! session state, and heartbeat logic.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod handshake;
pub mod manager;
pub mod time;

pub use handshake::{HelloPayload, HelloAckPayload, HandshakeError};
pub use manager::{Session, SessionManager, SessionState};
pub use time::{now_us, skew_ok, MAX_SKEW_US};
