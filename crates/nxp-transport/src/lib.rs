//! NXP transport layer.
//!
//! See RFC §2.1. NXP runs over QUIC, which provides TLS 1.3, multiplexed
//! streams, 0-RTT resumption, and connection migration out of the box.
//! This crate wraps the `quinn` QUIC implementation and provides:
//! - `NxpServer` — accepts incoming NXP connections
//! - `NxpClient` — establishes outgoing NXP sessions
//! - `NxpConnection` — frame-level read/write over a QUIC stream
//!
//! The TLS layer is configured for **self-signed certificates by default**,
//! which is appropriate for internal cluster communication where identity
//! is established at the NXP session layer (Ed25519 identity keys). For
//! external ingress, the API Gateway terminates TLS with publicly-trusted
//! certificates.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod cert;
pub mod conn;
pub mod server;
pub mod client;

pub use client::NxpClient;
pub use conn::{NxpConnection, ReadFrameError};
pub use server::NxpServer;
