//! Self-signed certificate generation for internal QUIC transport.
//!
//! For cluster-internal traffic, we use self-signed certificates because
//! peer identity is established at the NXP session layer (Ed25519 keys).
//! For external ingress, the API Gateway uses publicly-trusted certificates.

use quinn::crypto::rustls::QuicServerConfig;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::sync::Arc;

/// A self-signed TLS certificate + private key suitable for `quinn`.
pub struct SelfSignedCert {
    /// Certificate chain (DER-encoded).
    pub cert: CertificateDer<'static>,
    /// Private key (DER-encoded PKCS#8).
    pub key: PrivateKeyDer<'static>,
}

impl std::fmt::Debug for SelfSignedCert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelfSignedCert")
            .field("cert_len", &self.cert.len())
            .field("key_len", &self.key.secret_der().len())
            .finish()
    }
}

/// Generate a fresh self-signed certificate for the given subject CN.
pub fn generate(cn: &str) -> Result<SelfSignedCert, rcgen::Error> {
    let mut params = CertificateParams::new(vec![cn.to_string()])?;
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, cn);

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivatePkcs8KeyDer::from(key_pair.serialize_der()).into();
    Ok(SelfSignedCert {
        cert: cert_der,
        key: key_der,
    })
}

/// Build a `quinn::ServerConfig` that accepts any self-signed certificate
/// (no client cert verification — peer identity is checked at NXP layer).
pub fn server_config(
    cert: &SelfSignedCert,
) -> Result<quinn::ServerConfig, rustls::Error> {
    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert.cert.clone()], cert.key.clone_key())?;
    server_crypto.alpn_protocols = vec![b"nxp/1".to_vec()];
    let quic_server = QuicServerConfig::try_from(server_crypto)
        .map_err(|e| rustls::Error::General(e.to_string()))?;
    Ok(quinn::ServerConfig::with_crypto(Arc::new(quic_server)))
}

/// Build a `rustls::ClientConfig` that skips server cert verification
/// (for internal cluster traffic only).
pub fn client_config_skip_verify() -> Arc<rustls::ClientConfig> {
    let mut client_crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerify))
        .with_no_client_auth();
    client_crypto.alpn_protocols = vec![b"nxp/1".to_vec()];
    Arc::new(client_crypto)
}

#[derive(Debug)]
struct NoVerify;

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_self_signed() {
        let cert = generate("test.nexora.internal").unwrap();
        assert!(cert.cert.len() > 100);
        assert!(cert.key.secret_der().len() > 100);
    }

    #[test]
    fn server_config_builds() {
        let cert = generate("test.nexora.internal").unwrap();
        let _cfg = server_config(&cert).unwrap();
    }
}
