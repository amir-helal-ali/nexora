//! NXP server — accepts incoming QUIC connections.

use crate::cert::{generate, server_config, SelfSignedCert};
use crate::conn::NxpConnection;
use quinn::Endpoint;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;

/// NXP server. Owns a `quinn::Endpoint` that listens for incoming QUIC
/// connections and yields `NxpConnection`s.
pub struct NxpServer {
    /// Underlying QUIC endpoint.
    pub endpoint: Endpoint,
    /// Self-signed certificate used (for observability).
    pub cert: Arc<SelfSignedCert>,
}

impl fmt::Debug for NxpServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NxpServer")
            .field("local_addr", &self.endpoint.local_addr().ok())
            .finish_non_exhaustive()
    }
}

/// Error type for server bind.
pub type ServerError = anyhow::Error;

impl NxpServer {
    /// Bind a new NXP server to the given address. Generates a fresh
    /// self-signed certificate.
    pub async fn bind(addr: SocketAddr) -> Result<Self, ServerError> {
        let cert = generate("nexora.internal")?;
        let server_crypto = server_config(&cert)?;
        let endpoint = Endpoint::server(server_crypto, addr)?;
        Ok(Self {
            endpoint,
            cert: Arc::new(cert),
        })
    }

    /// Local address the server is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.endpoint.local_addr()
    }

    /// Wait for the next incoming connection. Returns the `NxpConnection`
    /// once the QUIC handshake completes and a bidirectional stream is opened
    /// by the peer.
    pub async fn accept(&self) -> Option<NxpConnection> {
        let incoming = self.endpoint.accept().await?;
        let conn = incoming.await.ok()?;
        let (send, recv) = conn.accept_bi().await.ok()?;
        Some(NxpConnection::from_streams(send, recv))
    }

    /// Close the server endpoint.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"server shutdown");
    }
}
