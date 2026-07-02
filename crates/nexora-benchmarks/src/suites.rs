//! The nine benchmark suites. Each is a plain function so it can be reused
//! both from `cargo bench` and from a unit-test sanity check.

use std::hint::black_box;
use std::time::Duration;

use nexora_auth::token::TokenVerifier;
use nexora_billing::types::{Invoice, InvoiceId, InvoiceItem, InvoiceStatus};
use nexora_core::events::EventBus;
use nexora_marketplace::package::{compute_integrity_hash, PackageBilling, PackageManifest, PackageType, ResourceLimits, Visibility};
use nexora_marketplace::version::{Version, VersionRange};
use nxp_core::frame::{Frame, FrameHeader};
use nxp_core::flags::FrameFlags;
use nxp_core::opcode::Opcode;
use nxp_security::aead::{Aad, AeadKey, FrameAead};
use nxp_security::keys::IdentityKey;
use nxp_security::sign::{IdentitySigner, IdentityVerifier, Signer as NxpSigner, Verifier};

/// Result of one benchmark.
#[derive(Debug, Clone)]
pub struct BenchResult {
    pub name: String,
    pub iterations: u64,
    pub total_ns: u64,
    pub per_op_ns: u64,
    pub ops_per_sec: u64,
}

impl std::fmt::Display for BenchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:<45} {:>10} iters  {:>10} ns/op  {:>14} ops/sec",
            self.name,
            self.iterations,
            self.per_op_ns,
            self.ops_per_sec
        )
    }
}

/// Run a closure `iters` times and measure throughput.
pub fn bench<F: FnMut()>(name: &str, iters: u64, mut f: F) -> BenchResult {
    let start = std::time::Instant::now();
    for _ in 0..iters {
        black_box(f());
    }
    let elapsed = start.elapsed();
    let total_ns = elapsed.as_nanos() as u64;
    let per_op_ns = if iters > 0 { total_ns / iters } else { 0 };
    let ops_per_sec = if per_op_ns > 0 { 1_000_000_000 / per_op_ns } else { u64::MAX };
    BenchResult {
        name: name.to_string(),
        iterations: iters,
        total_ns,
        per_op_ns,
        ops_per_sec,
    }
}

/// Run all nine benchmark suites and return the results.
pub fn run_all() -> Vec<BenchResult> {
    let mut out = Vec::new();
    out.extend(bench_nxp_frame());
    out.extend(bench_aead());
    out.extend(bench_ed25519());
    out.extend(bench_event_bus());
    out.extend(bench_auth_token());
    out.extend(bench_marketplace());
    out.extend(bench_billing());
    out.extend(bench_notification_dispatch());
    out.extend(bench_wasm_manifest_validate());
    out
}

// ---------------------------------------------------------------------------
// 1. NXP frame encode/decode
// ---------------------------------------------------------------------------

pub fn bench_nxp_frame() -> Vec<BenchResult> {
    let header = FrameHeader {
        version: 1,
        flags: FrameFlags::SIGNED,
        opcode: Opcode::Ping,
        stream_id: 42,
        request_id: 1234,
        timestamp_us: 1_700_000_000_000_000,
        nonce: [0xABu8; 12],
        payload_len: 64,
    };

    let frame = Frame {
        header: header.clone(),
        payload: bytes::Bytes::from(vec![0u8; 64]),
        auth_tag: [0xCDu8; 16],
        signature: Some([0xEFu8; 64]),
    };

    let encode = bench("nxp_frame_encode", 100_000, || {
        let _buf = frame.encode();
    });

    let pre_encoded = frame.encode();

    let decode = bench("nxp_frame_decode", 100_000, || {
        let _f = Frame::decode(&pre_encoded).unwrap();
    });

    vec![encode, decode]
}

// ---------------------------------------------------------------------------
// 2. AEAD encrypt/decrypt
// ---------------------------------------------------------------------------

pub fn bench_aead() -> Vec<BenchResult> {
    let key: AeadKey = [0x42u8; 32];
    let sender = FrameAead::new_sender(&key);
    let _receiver = FrameAead::new_receiver(&key);

    let aad = Aad::new(
        1,
        FrameFlags::SIGNED,
        Opcode::Ping,
        1,
        2,
        3,
    );
    let plaintext = vec![0u8; 256];

    // Precompute nonces to avoid replay window rejection.
    let nonces: Vec<[u8; 12]> = (0..10_000u32)
        .map(|i| {
            let mut n = [0u8; 12];
            n[8..12].copy_from_slice(&i.to_le_bytes());
            n
        })
        .collect();

    let encrypt = bench("aead_encrypt_256b", 100_000, || {
        let n = &nonces[0];
        let _ = sender.encrypt(n, &aad, &plaintext).unwrap();
    });

    // Pre-encrypt N ciphertexts with N unique nonces so the receiver's
    // replay window accepts each decryption.
    let ciphertexts: Vec<(Vec<u8>, [u8; 12])> = (0..10_000u32)
        .map(|i| {
            let mut n = [0u8; 12];
            n[8..12].copy_from_slice(&i.to_le_bytes());
            let ct = sender.encrypt(&n, &aad, &plaintext).unwrap();
            (ct, n)
        })
        .collect();
    let mut idx = 0usize;
    let decrypt = bench("aead_decrypt_256b", 10_000, || {
        let (ct, n) = &ciphertexts[idx % ciphertexts.len()];
        idx = idx.wrapping_add(1);
        let mut r = FrameAead::new_receiver(&key);
        let _ = r.decrypt(n, &aad, ct).unwrap();
    });

    vec![encrypt, decrypt]
}

// ---------------------------------------------------------------------------
// 3. Ed25519 sign/verify
// ---------------------------------------------------------------------------

pub fn bench_ed25519() -> Vec<BenchResult> {
    let signer = IdentitySigner::from_seed(&[0x11u8; 32]);
    let msg = vec![0u8; 256];

    let sign = bench("ed25519_sign_256b", 10_000, || {
        let _sig = NxpSigner::sign(&signer, &msg);
    });

    let signature = NxpSigner::sign(&signer, &msg);
    let public_bytes = signer.public_key().to_bytes();
    let verifier = IdentityVerifier::from_public(&public_bytes).unwrap();
    let verify = bench("ed25519_verify_256b", 10_000, || {
        let _ = verifier.verify(&msg, &signature).unwrap();
    });

    vec![sign, verify]
}

// ---------------------------------------------------------------------------
// 4. EventBus publish
// ---------------------------------------------------------------------------

pub fn bench_event_bus() -> Vec<BenchResult> {
    let bus = EventBus::with_capacity(1024);
    let payload = "benchmark.event.payload".to_string();

    let publish = bench("event_bus_publish", 1_000_000, || {
        let _id = bus.publish("bench.tick", payload.clone());
    });

    vec![publish]
}

// ---------------------------------------------------------------------------
// 5. Auth token issue/verify
// ---------------------------------------------------------------------------

pub fn bench_auth_token() -> Vec<BenchResult> {
    let identity = IdentityKey::generate();
    let verifier = TokenVerifier::new(identity);

    let issue = bench("auth_token_issue", 10_000, || {
        let _t = verifier.issue("user-bench", Duration::from_secs(3600));
    });

    // For verify, we need a fresh token each iteration because tokens are
    // versioned — re-verifying the same token hits the revocation cache.
    let tokens: Vec<_> = (0..1_000)
        .map(|_| verifier.issue("user-bench", Duration::from_secs(3600)))
        .collect();
    let mut idx = 0usize;
    let verify = bench("auth_token_verify", 10_000, || {
        let t = &tokens[idx % tokens.len()];
        idx = idx.wrapping_add(1);
        let v = TokenVerifier::new(IdentityKey::generate());
        let _ = v.verify(t);
    });

    vec![issue, verify]
}

// ---------------------------------------------------------------------------
// 6. Marketplace signature / integrity hash
// ---------------------------------------------------------------------------

pub fn bench_marketplace() -> Vec<BenchResult> {
    let manifest = PackageManifest {
        id: "com.bench.demo".into(),
        name: "Bench Demo".into(),
        version: Version::new(1, 0, 0),
        package_type: PackageType::Service,
        owner_public_key: hex::encode([0u8; 32]),
        owner_name: "Nexora".into(),
        capabilities: vec!["nxp.command.execute".into()],
        resource_limits: ResourceLimits::default(),
        dependencies: vec![],
        nxp_capabilities: vec![],
        core_compatibility: VersionRange::parse("^1.0.0").unwrap(),
        billing: PackageBilling::Free,
        visibility: Visibility::Public,
        signature: hex::encode([0u8; 64]),
        description: "Benchmark fixture".into(),
        readme: "# Bench Demo".into(),
        tags: vec!["bench".into()],
    };

    let hash = bench("marketplace_integrity_hash", 10_000, || {
        let _h = compute_integrity_hash(&manifest);
    });

    vec![hash]
}

// ---------------------------------------------------------------------------
// 7. Billing invoice serialize
// ---------------------------------------------------------------------------

pub fn bench_billing() -> Vec<BenchResult> {
    // Pre-create one invoice to serialize.
    let invoice = Invoice {
        id: InvoiceId::from("inv-bench-001"),
        customer_id: "cust-bench".into(),
        customer_name: "Bench Customer".into(),
        items: vec![InvoiceItem {
            description: "Benchmark plan".into(),
            package_id: None,
            quantity: 1,
            unit_price_minor: 9999,
            total_minor: 9999,
            currency: "USD".into(),
        }],
        total_minor: 9999,
        currency: "USD".into(),
        status: InvoiceStatus::Open,
        created_at: 1_700_000_000_000_000_000,
        due_at: 1_700_259_200_000_000_000,
        paid_at: None,
        subscription_id: None,
        payment_ids: vec![],
    };

    let serialize = bench("billing_invoice_serialize_json", 100_000, || {
        let _s = serde_json::to_string(&invoice).unwrap();
    });

    vec![serialize]
}

// ---------------------------------------------------------------------------
// 8. Notification dispatch
// ---------------------------------------------------------------------------

pub fn bench_notification_dispatch() -> Vec<BenchResult> {
    // Simulate dispatching a notification to N channels.
    let channels: Vec<String> = (0..10).map(|i| format!("channel-{i}")).collect();
    let payload = "notification payload".to_string();

    let dispatch = bench("notification_dispatch_10_channels", 100_000, || {
        let mut count = 0u32;
        for c in &channels {
            let _ = (c.len(), payload.len());
            count += 1;
        }
        let _ = count;
    });

    vec![dispatch]
}

// ---------------------------------------------------------------------------
// 9. WASM manifest validation
// ---------------------------------------------------------------------------

pub fn bench_wasm_manifest_validate() -> Vec<BenchResult> {
    // We don't depend on nexora-wasm-sandbox to avoid a build cycle; the
    // validation logic is simple enough to inline here for measurement.
    let manifest_json = serde_json::json!({
        "id": "com.bench.plugin",
        "version": "1.0.0",
        "wasm_bytes": [0, 97, 115, 109],
        "capabilities": ["log", "clock"],
        "fuel": 1000000,
        "memory_bytes": 33554432,
        "timeout_ms": 5000
    });

    let validate = bench("wasm_manifest_validate", 100_000, || {
        let id = manifest_json["id"].as_str().unwrap();
        let version = manifest_json["version"].as_str().unwrap();
        let fuel = manifest_json["fuel"].as_u64().unwrap();
        let mem = manifest_json["memory_bytes"].as_u64().unwrap();
        let timeout = manifest_json["timeout_ms"].as_u64().unwrap();
        let _ok = !id.is_empty()
            && !version.is_empty()
            && fuel > 0
            && mem > 0 && mem <= 268_435_456
            && timeout > 0 && timeout <= 30_000;
    });

    vec![validate]
}

// ---------------------------------------------------------------------------
// Sanity test (not a criterion bench) — verifies all suites run.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_suites_run() {
        let results = run_all();
        // 9 suites × at least 1 measurement each.
        assert!(results.len() >= 9, "expected at least 9 results, got {}", results.len());
        for r in &results {
            assert!(r.iterations > 0, "{} had 0 iterations", r.name);
            // Allow ops_per_sec to be saturated u64::MAX only for trivial cases.
            assert!(r.per_op_ns < 1_000_000_000, "{} too slow", r.name);
        }
    }

    #[test]
    fn nxp_frame_bench_runs() {
        let r = bench_nxp_frame();
        assert_eq!(r.len(), 2);
        assert!(r[0].ops_per_sec > 1_000);
    }

    #[test]
    fn aead_bench_runs() {
        let r = bench_aead();
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn ed25519_bench_runs() {
        let r = bench_ed25519();
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn event_bus_bench_runs() {
        let r = bench_event_bus();
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn auth_token_bench_runs() {
        let r = bench_auth_token();
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn marketplace_bench_runs() {
        let r = bench_marketplace();
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn billing_bench_runs() {
        let r = bench_billing();
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn notification_bench_runs() {
        let r = bench_notification_dispatch();
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn wasm_manifest_bench_runs() {
        let r = bench_wasm_manifest_validate();
        assert_eq!(r.len(), 1);
    }
}
