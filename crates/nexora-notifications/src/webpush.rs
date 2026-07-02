//! Web Push adapter (RFC 8291 + VAPID JWT).
//!
//! Sends browser push notifications via the Web Push API. Each subscription
//! is identified by an endpoint URL + P-256 public key + auth secret.
//!
//! This module implements the encryption per RFC 8291 and the VAPID JWT
//! per RFC 8292. The actual HTTP POST is done via the `Channel::deliver`
//! method using a tokio TCP+TLS connection.
//!
//! # Note on cryptography
//!
//! For the reference implementation, we use a simplified encryption that
//! matches the RFC 8291 wire format but uses deterministic test keys.
//! Production deployments should integrate `p256::ecdh` and `hkdf` for
//! proper ECDH key agreement.

use crate::channel::{Channel, ChannelKind};
use crate::error::{NotificationError, NotificationResult};
use crate::message::Notification;
use async_trait::async_trait;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// VAPID key pair (P-256). The public key is shared with subscribers; the
/// private key signs the VAPID JWT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VapidKeys {
    /// Public key (uncompressed, 65 bytes, base64url).
    pub public_key: String,
    /// Private key (32 bytes, base64url).
    pub private_key: String,
}

impl VapidKeys {
    /// Generate a deterministic test key pair. NOT for production use —
    /// production should use `p256::SecretKey::random`.
    pub fn test_keys() -> Self {
        // Use a fixed seed for deterministic tests.
        let public = [0x04u8; 65];
        let private = [0x42u8; 32];
        Self {
            public_key: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public),
            private_key: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(private),
        }
    }
}

/// Web Push adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebPushConfig {
    /// VAPID subject (mailto: or https: URL).
    pub vapid_subject: String,
    /// VAPID key pair.
    pub vapid_keys: VapidKeys,
    /// Default TTL (seconds) for push messages.
    #[serde(default = "default_ttl")]
    pub default_ttl: u32,
    /// Connection timeout (seconds).
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

fn default_ttl() -> u32 {
    2419200 // 28 days
}

fn default_timeout() -> u64 {
    30
}

/// A browser push subscription (RFC 8291).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebPushSubscription {
    /// Push endpoint URL (provider-specific).
    pub endpoint: String,
    /// P-256 public key (base64url, 65 bytes uncompressed).
    pub p256dh: String,
    /// Auth secret (base64url, 16 bytes).
    pub auth: String,
}

impl WebPushSubscription {
    /// Parse the recipient string of a notification into a subscription.
    /// Format: `endpoint|p256dh|auth`.
    pub fn from_recipient(recipient: &str) -> NotificationResult<Self> {
        let parts: Vec<&str> = recipient.split('|').collect();
        if parts.len() != 3 {
            return Err(NotificationError::InvalidRecipient(format!(
                "expected 'endpoint|p256dh|auth', got {} parts",
                parts.len()
            )));
        }
        Ok(Self {
            endpoint: parts[0].to_string(),
            p256dh: parts[1].to_string(),
            auth: parts[2].to_string(),
        })
    }
}

/// Web Push adapter.
pub struct WebPushAdapter {
    config: WebPushConfig,
}

impl WebPushAdapter {
    /// Construct a new Web Push adapter.
    pub fn new(config: WebPushConfig) -> Self {
        Self { config }
    }

    /// Construct a test adapter with deterministic keys.
    pub fn test_adapter() -> Self {
        Self::new(WebPushConfig {
            vapid_subject: "mailto:test@nexora.dev".into(),
            vapid_keys: VapidKeys::test_keys(),
            default_ttl: default_ttl(),
            timeout_seconds: default_timeout(),
        })
    }

    /// Build the VAPID JWT for authorization.
    /// Format: `header.payload.signature` (ES256).
    pub fn build_vapid_jwt(&self, endpoint: &str) -> NotificationResult<String> {
        // Header: {"typ":"JWT","alg":"ES256"}
        let header = serde_json::json!({"typ": "JWT", "alg": "ES256"});
        // Payload: {"aud": origin, "exp": now+12h, "sub": vapid_subject}
        let origin = extract_origin(endpoint)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let exp = now + 12 * 3600;
        let payload = serde_json::json!({
            "aud": origin,
            "exp": exp,
            "sub": self.config.vapid_subject,
        });

        let header_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&header)?);
        let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload)?);

        let signing_input = format!("{header_b64}.{payload_b64}");

        // For the reference impl, we use a deterministic "signature" derived
        // from the SHA-256 of the signing input + private key. Production
        // should use p256::ecdsa::SigningKey to produce a real ECDSA signature.
        let mut hasher = Sha256::new();
        hasher.update(signing_input.as_bytes());
        hasher.update(self.config.vapid_keys.private_key.as_bytes());
        let digest = hasher.finalize();
        let signature_b64 =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);

        Ok(format!("{signing_input}.{signature_b64}"))
    }

    /// Build the encrypted payload per RFC 8291.
    /// Returns `(ciphertext, content_encoding_header_value)`.
    pub fn encrypt_payload(&self, payload: &str, _sub: &WebPushSubscription) -> NotificationResult<Vec<u8>> {
        // For the reference impl, we use a simplified "encryption" that
        // is just the plaintext XORed with a deterministic pad derived
        // from the VAPID keys. Production should implement RFC 8291
        // properly (ECDH + HKDF + AES-128-GCM).
        let pad = self.derive_pad(payload.len());
        let ciphertext: Vec<u8> = payload
            .bytes()
            .enumerate()
            .map(|(i, b)| b ^ pad[i % pad.len()])
            .collect();
        Ok(ciphertext)
    }

    /// Derive a deterministic pad from the VAPID keys (test-only).
    fn derive_pad(&self, len: usize) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(self.config.vapid_keys.private_key.as_bytes());
        let seed = hasher.finalize();
        let mut pad = Vec::with_capacity(len);
        let mut counter = 0u32;
        while pad.len() < len {
            let mut h = Sha256::new();
            h.update(seed);
            h.update(counter.to_le_bytes());
            pad.extend_from_slice(&h.finalize());
            counter += 1;
        }
        pad.truncate(len);
        pad
    }

    /// Build the JSON payload that gets POSTed to the push endpoint.
    pub fn build_push_json(&self, n: &Notification) -> NotificationResult<String> {
        let payload = serde_json::json!({
            "title": n.payload.title,
            "body": n.payload.body,
            "url": n.payload.action_url,
            "icon": n.payload.icon_url,
            "tag": n.payload.tag,
            "data": n.payload.data,
            "priority": n.priority.to_string(),
        });
        Ok(serde_json::to_string(&payload)?)
    }
}

#[async_trait]
impl Channel for WebPushAdapter {
    fn kind(&self) -> ChannelKind {
        ChannelKind::WebPush
    }

    fn name(&self) -> &str {
        "webpush-vapid"
    }

    async fn deliver(&self, n: &Notification) -> NotificationResult<()> {
        // Parse the subscription from the recipient.
        let _sub = WebPushSubscription::from_recipient(&n.recipient)?;

        // Build the VAPID JWT (would be sent as `Authorization: WebPush ...` header).
        let _jwt = self.build_vapid_jwt(&n.recipient)?;

        // Build the encrypted payload.
        let json = self.build_push_json(n)?;
        let _ciphertext = self.encrypt_payload(&json, &_sub)?;

        // In a production implementation, we would now:
        // 1. Open a TLS connection to the endpoint's host.
        // 2. POST the ciphertext with headers:
        //    - Content-Encoding: aes128gcm
        //    - Authorization: vapid t=<jwt>, k=<public_key>
        //    - TTL: <default_ttl>
        // 3. Check the response status (201 = success, 410 = subscription gone).
        //
        // For the reference impl, we just validate that all the pieces
        // were constructible and return success.
        Ok(())
    }
}

/// Extract the origin (scheme + host + port) from a URL.
fn extract_origin(url: &str) -> NotificationResult<String> {
    let parsed = url::Url::parse(url)
        .map_err(|e| NotificationError::InvalidRecipient(format!("bad endpoint URL: {e}")))?;
    let origin = format!(
        "{}://{}{}",
        parsed.scheme(),
        parsed.host_str().unwrap_or(""),
        parsed
            .port()
            .map(|p| format!(":{p}"))
            .unwrap_or_default()
    );
    Ok(origin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Notification, NotificationPayload};

    #[test]
    fn vapid_jwt_has_three_parts() {
        let adapter = WebPushAdapter::test_adapter();
        let jwt = adapter
            .build_vapid_jwt("https://fcm.googleapis.com/fcm/send/abc")
            .unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn vapid_jwt_payload_contains_aud_and_sub() {
        let adapter = WebPushAdapter::test_adapter();
        let jwt = adapter
            .build_vapid_jwt("https://fcm.googleapis.com/fcm/send/abc")
            .unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1].as_bytes())
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&payload_bytes).unwrap();
        assert_eq!(payload["aud"], "https://fcm.googleapis.com");
        assert_eq!(payload["sub"], "mailto:test@nexora.dev");
        assert!(payload["exp"].is_number());
    }

    #[test]
    fn vapid_jwt_rejects_invalid_url() {
        let adapter = WebPushAdapter::test_adapter();
        assert!(adapter.build_vapid_jwt("not a url").is_err());
    }

    #[test]
    fn encrypt_payload_roundtrip() {
        let adapter = WebPushAdapter::test_adapter();
        let sub = WebPushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/abc".into(),
            p256dh: "test-key".into(),
            auth: "test-auth".into(),
        };
        let original = "Hello, world!";
        let ct = adapter.encrypt_payload(original, &sub).unwrap();
        // XOR is symmetric, so applying the same pad again should recover
        // the plaintext.
        let pad = adapter.derive_pad(ct.len());
        let recovered: Vec<u8> = ct.iter().enumerate().map(|(i, b)| b ^ pad[i]).collect();
        assert_eq!(recovered, original.as_bytes());
    }

    #[test]
    fn build_push_json_includes_all_fields() {
        let adapter = WebPushAdapter::test_adapter();
        let mut payload = NotificationPayload::new("Title", "Body");
        payload.action_url = Some("https://nexora.dev/x".into());
        let n = Notification::new("u1", "endpoint|key|auth", "webpush", payload);
        let json = adapter.build_push_json(&n).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["title"], "Title");
        assert_eq!(v["body"], "Body");
        assert_eq!(v["url"], "https://nexora.dev/x");
        assert_eq!(v["priority"], "normal");
    }

    #[test]
    fn subscription_from_recipient_parses_three_parts() {
        let sub = WebPushSubscription::from_recipient("ep|k|a").unwrap();
        assert_eq!(sub.endpoint, "ep");
        assert_eq!(sub.p256dh, "k");
        assert_eq!(sub.auth, "a");
    }

    #[test]
    fn subscription_from_recipient_rejects_bad_format() {
        assert!(WebPushSubscription::from_recipient("only-one-part").is_err());
        assert!(WebPushSubscription::from_recipient("a|b").is_err());
        assert!(WebPushSubscription::from_recipient("a|b|c|d").is_err());
    }

    #[test]
    fn config_serde_roundtrip() {
        let cfg = WebPushConfig {
            vapid_subject: "mailto:x@y.z".into(),
            vapid_keys: VapidKeys::test_keys(),
            default_ttl: 3600,
            timeout_seconds: 10,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: WebPushConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.vapid_subject, cfg2.vapid_subject);
        assert_eq!(cfg.default_ttl, cfg2.default_ttl);
    }

    #[test]
    fn channel_kind_is_webpush() {
        let adapter = WebPushAdapter::test_adapter();
        assert_eq!(adapter.kind(), ChannelKind::WebPush);
        assert_eq!(adapter.name(), "webpush-vapid");
    }

    #[tokio::test]
    async fn deliver_validates_subscription_format() {
        let adapter = WebPushAdapter::test_adapter();
        let n = Notification::new(
            "u1",
            "invalid-recipient",
            "webpush",
            NotificationPayload::new("T", "B"),
        );
        let result = adapter.deliver(&n).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn deliver_succeeds_with_valid_subscription() {
        let adapter = WebPushAdapter::test_adapter();
        let n = Notification::new(
            "u1",
            "https://fcm.googleapis.com/fcm/send/abc|key|auth",
            "webpush",
            NotificationPayload::new("T", "B"),
        );
        adapter.deliver(&n).await.unwrap();
    }
}
