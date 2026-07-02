//! # Nexora Performance Benchmarks
//!
//! Nine benchmark suites measuring the hot paths of the platform.
//!
//! ## Suites
//!
//! 1. `nxp_frame_encode` / `nxp_frame_decode` — NXP wire format
//! 2. `aead_encrypt` / `aead_decrypt` — ChaCha20-Poly1305
//! 3. `ed25519_sign` / `ed25519_verify` — signature throughput
//! 4. `event_publish` — EventBus fan-out
//! 5. `auth_token_issue` / `auth_token_verify` — token roundtrip
//! 6. `marketplace_signature_verify` — package signature check
//! 7. `billing_invoice_serialize` — billing JSON encoding
//! 8. `notification_dispatch` — notification routing
//! 9. `wasm_manifest_validate` — plugin manifest validation
//!
//! Run with: `cargo bench -p nexora-benchmarks`

pub mod suites;

pub use suites::run_all;
