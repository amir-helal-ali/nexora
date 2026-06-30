//! NXP client — establishes outgoing QUIC connections.

use crate::cert::client_config_skip_verify;
use crate::conn::NxpConnection;
use quinn::crypto::rustls::QuicClientConfig;
use quinn::Endpoint;
use std::net::SocketAddr;
use std::sync::Arc;

/// NXP client. Owns a `quinn::Endpoint` for outgoing connections.
pub struct NxpClient {
    endpoint: Endpoint,
}

impl std::fmt::Debug for NxpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NxpClient")
            .field("local_addr", &self.endpoint.local_addr().ok())
            .finish_non_exhaustive()
    }
}

/// Error type for client operations.
pub type ClientError = anyhow::Error;

impl NxpClient {
    /// Construct a new NXP client with a fresh ephemeral UDP socket.
    pub fn new() -> Result<Self, ClientError> {
        let client_crypto = client_config_skip_verify();
        let quic_client = QuicClientConfig::try_from(client_crypto)?;
        let client_config = quinn::ClientConfig::new(Arc::new(quic_client));
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())?;
        endpoint.set_default_client_config(client_config);
        Ok(Self { endpoint })
    }

    /// Connect to a remote NXP server and open a bidirectional stream.
    pub async fn connect(
        &self,
        server_addr: SocketAddr,
        server_name: &str,
    ) -> Result<NxpConnection, ClientError> {
        let conn = self.endpoint.connect(server_addr, server_name)?.await?;
        let (send, recv) = conn.open_bi().await?;
        Ok(NxpConnection::from_streams(send, recv))
    }

    /// Returns a reference to the underlying endpoint (for advanced use).
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

impl Default for NxpClient {
    fn default() -> Self {
        Self::new().expect("failed to construct default NxpClient")
    }
}
