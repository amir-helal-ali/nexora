//! # مقاييس أداء Nexora
//!
//! تسع مجموعات قياس تقيس المسارات الساخنة للمنصة.
//!
//! ## المجموعات
//!
//! 1. `nxp_frame_encode` / `nxp_frame_decode` — صيغة سلك NXP
//! 2. `aead_encrypt` / `aead_decrypt` — ChaCha20-Poly1305
//! 3. `ed25519_sign` / `ed25519_verify` — إنتاجية التوقيع
//! 4. `event_publish` — توزيع ناقل الأحداث
//! 5. `auth_token_issue` / `auth_token_verify` — ذهاب-إياب الرمز
//! 6. `marketplace_signature_verify` — فحص توقيع الحزمة
//! 7. `billing_invoice_serialize` — ترميز JSON للفوترة
//! 8. `notification_dispatch` — توجيه الإشعارات
//! 9. `wasm_manifest_validate` — التحقق من بيان المكون
//!
//! شغّل بـ: `cargo bench -p nexora-benchmarks`

pub mod suites;

pub use suites::run_all;
