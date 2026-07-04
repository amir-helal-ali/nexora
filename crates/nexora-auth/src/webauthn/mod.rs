//! # WebAuthn — مفاتيح الأمان المادية
//!
//! تنفيذ مبسّط لـ WebAuthn (Web Authentication) لدعم المفاتيح الأمنية
//! المادية مثل YubiKey و Google Titan و Windows Hello.
//!
//! # كيف يعمل
//!
//! ## التسجيل (Registration)
//! 1. الخادم يولّد تحدياً (challenge) عشوائياً
//! 2. العميل يطلب من المستخدم لمس المفتاح الأمني
//! 3. المفتاح يولّد زوج مفاتيح (public/private) جديداً
//! 4. المفتاح يوقّع التحدي بالمفتاح الخاص
//! 5. الخادم يتحقق من التوقيع بالمفتاح العام
//! 6. الخادم يخزّن المفتاح العام للمستخدم
//!
//! ## المصادقة (Authentication)
//! 1. الخادم يولّد تحدياً جديداً
//! 2. العميل يطلب من المستخدم لمس المفتاح
//! 3. المفتاح يوقّع التحدي
//! 4. الخادم يتحقق من التوقيع
//!
//! # ملاحظة
//!
//! هذا تنفيذ مرجعي للهيكل والمنطق. الإنتاج الكامل يتطلب:
//! - CBOR decoding لبيانات العميل (authenticatorData)
//! - التحقق من شهادات attestation
//! - دعم PIN protocol (CTAP 2.1)

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use time::OffsetDateTime;

/// تحدي WebAuthn (challenge).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    /// قيمة التحدي (base64url).
    pub challenge: String,
    /// وقت الإنشاء (unix nanos).
    pub created_at: i64,
    /// هل استُخدم؟ (لمنع إعادة الاستخدام).
    pub used: bool,
}

impl Challenge {
    /// توليد تحدي عشوائي جديد (32 بايت).
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        use base64::Engine;
        Self {
            challenge: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes),
            created_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            used: false,
        }
    }

    /// هل انتهت صلاحية التحدي؟ (5 دقائق).
    pub fn is_expired(&self) -> bool {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let age_secs = (now - self.created_at) / 1_000_000_000;
        age_secs > 300
    }
}

/// بيانات اعتماد مسجّلة (credential).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    /// معرّف بيانات الاعتماد (base64url).
    pub id: String,
    /// المفتاح العام (base64url).
    pub public_key: String,
    /// نوع المنشئ (authenticator).
    pub authenticator_type: String,
    /// اسم وصفي (يعطيه المستخدم).
    pub label: String,
    /// وقت التسجيل.
    pub created_at: i64,
    /// آخر استخدام.
    pub last_used_at: Option<i64>,
    /// عدد الاستخدامات.
    pub sign_count: u64,
}

/// نتيجة التسجيل.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationResult {
    pub credential_id: String,
    pub label: String,
}

/// مدير WebAuthn لمستخدم واحد.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserWebAuthnState {
    /// بيانات الاعتماد المسجّلة.
    pub credentials: Vec<Credential>,
    /// التحديات المعلّقة.
    pub pending_challenges: Vec<Challenge>,
}

/// مدير WebAuthn العام.
pub struct WebAuthnManager {
    /// حالة كل مستخدم.
    states: RwLock<HashMap<String, UserWebAuthnState>>,
}

impl Default for WebAuthnManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WebAuthnManager {
    pub fn new() -> Self {
        Self {
            states: RwLock::new(HashMap::new()),
        }
    }

    /// بدء التسجيل — يولّد تحدياً للمستخدم.
    pub fn begin_registration(&self, user_id: &str) -> Challenge {
        let challenge = Challenge::generate();
        let mut states = self.states.write();
        let state = states
            .entry(user_id.to_string())
            .or_default();
        state.pending_challenges.push(challenge.clone());
        challenge
    }

    /// إكمال التسجيل — يتحقق من الاستجابة ويخزّن بيانات الاعتماد.
    pub fn complete_registration(
        &self,
        user_id: &str,
        challenge_response: &str,
        credential_id: &str,
        public_key: &str,
        authenticator_type: &str,
        label: &str,
    ) -> Result<RegistrationResult, String> {
        let mut states = self.states.write();
        let state = states
            .entry(user_id.to_string())
            .or_default();

        // ابحث عن التحدي المطابق.
        let challenge_idx = state
            .pending_challenges
            .iter()
            .position(|c| {
                // في التنفيذ المرجعي، نقبل أي تحدي غير منتهي.
                // في الإنتاج، نتحقق من أن challenge_response يحوي التحدي الصحيح.
                !c.used && !c.is_expired()
            })
            .ok_or_else(|| "لا يوجد تحدي صالح".to_string())?;

        // علّم التحدي كمستخدم.
        state.pending_challenges.remove(challenge_idx);

        // تحقق من عدم تكرار معرّف بيانات الاعتماد.
        if state.credentials.iter().any(|c| c.id == credential_id) {
            return Err("بيانات اعتماد مسجّلة بالفعل".into());
        }

        // خزّن بيانات الاعتماد.
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let credential = Credential {
            id: credential_id.to_string(),
            public_key: public_key.to_string(),
            authenticator_type: authenticator_type.to_string(),
            label: label.to_string(),
            created_at: now,
            last_used_at: None,
            sign_count: 0,
        };
        state.credentials.push(credential.clone());

        Ok(RegistrationResult {
            credential_id: credential.id,
            label: credential.label,
        })
    }

    /// بدء المصادقة — يولّد تحدياً جديداً.
    pub fn begin_authentication(&self, user_id: &str) -> Result<Challenge, String> {
        let mut states = self.states.write();
        let state = states
            .entry(user_id.to_string())
            .or_default();

        if state.credentials.is_empty() {
            return Err("لا توجد بيانات اعتماد مسجّلة".into());
        }

        let challenge = Challenge::generate();
        state.pending_challenges.push(challenge.clone());
        Ok(challenge)
    }

    /// إكمال المصادقة — يتحقق من التوقيع.
    pub fn complete_authentication(
        &self,
        user_id: &str,
        credential_id: &str,
        _signature: &str,
        sign_count: u64,
    ) -> Result<(), String> {
        let mut states = self.states.write();
        let state = states
            .entry(user_id.to_string())
            .or_default();

        // استهلك تحدياً معلّقاً.
        let challenge_idx = state
            .pending_challenges
            .iter()
            .position(|c| !c.used && !c.is_expired())
            .ok_or_else(|| "لا يوجد تحدي صالح".to_string())?;
        state.pending_challenges.remove(challenge_idx);

        // ابحث عن بيانات الاعتماد.
        let cred = state
            .credentials
            .iter_mut()
            .find(|c| c.id == credential_id)
            .ok_or_else(|| "بيانات اعتماد غير موجودة".to_string())?;

        // تحقق من sign count (منع إعادة التشغيل).
        if sign_count <= cred.sign_count && cred.sign_count > 0 {
            return Err("sign count غير صالح (احتمال استنساخ)".into());
        }

        cred.sign_count = sign_count;
        cred.last_used_at = Some(OffsetDateTime::now_utc().unix_timestamp_nanos() as i64);

        // في الإنتاج، نتحقق من التوقيع فعلياً باستخدام المفتاح العام.
        // للتنفيذ المرجعي، نقبل أي توقيع غير فارغ.
        if _signature.is_empty() {
            return Err("توقيع فارغ".into());
        }

        Ok(())
    }

    /// هل المستخدم لديه بيانات اعتماد WebAuthn؟
    pub fn is_registered(&self, user_id: &str) -> bool {
        self.states
            .read()
            .get(user_id)
            .map(|s| !s.credentials.is_empty())
            .unwrap_or(false)
    }

    /// قائمة بيانات الاعتماد لمستخدم.
    pub fn list_credentials(&self, user_id: &str) -> Vec<Credential> {
        self.states
            .read()
            .get(user_id)
            .map(|s| s.credentials.clone())
            .unwrap_or_default()
    }

    /// حذف بيانات اعتماد.
    pub fn remove_credential(&self, user_id: &str, credential_id: &str) -> bool {
        let mut states = self.states.write();
        if let Some(state) = states.get_mut(user_id) {
            let before = state.credentials.len();
            state.credentials.retain(|c| c.id != credential_id);
            state.credentials.len() != before
        } else {
            false
        }
    }

    /// عدد المستخدمين المسجّلين.
    pub fn registered_count(&self) -> usize {
        self.states
            .read()
            .values()
            .filter(|s| !s.credentials.is_empty())
            .count()
    }

    /// تنظيف التحديات المنتهية.
    pub fn cleanup_expired_challenges(&self) -> usize {
        let mut states = self.states.write();
        let mut total = 0;
        for state in states.values_mut() {
            let before = state.pending_challenges.len();
            state.pending_challenges.retain(|c| !c.is_expired());
            total += before - state.pending_challenges.len();
        }
        total
    }
}

/// تجزئة بيانات لتوليد معرّف ثابت.
pub fn hash_data(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_generate_is_unique() {
        let c1 = Challenge::generate();
        let c2 = Challenge::generate();
        assert_ne!(c1.challenge, c2.challenge);
        assert!(!c1.challenge.is_empty());
    }

    #[test]
    fn challenge_not_expired_immediately() {
        let c = Challenge::generate();
        assert!(!c.is_expired());
    }

    #[test]
    fn begin_registration_creates_challenge() {
        let mgr = WebAuthnManager::new();
        let challenge = mgr.begin_registration("alice");
        assert!(!challenge.challenge.is_empty());
    }

    #[test]
    fn complete_registration_succeeds() {
        let mgr = WebAuthnManager::new();
        let _challenge = mgr.begin_registration("alice");
        let result = mgr
            .complete_registration(
                "alice",
                "response",
                "cred-1",
                "pub-key-1",
                "yubikey",
                "مفتاحي",
            )
            .unwrap();
        assert_eq!(result.credential_id, "cred-1");
        assert!(mgr.is_registered("alice"));
    }

    #[test]
    fn complete_registration_no_challenge_fails() {
        let mgr = WebAuthnManager::new();
        let result = mgr.complete_registration(
            "alice",
            "r",
            "c",
            "p",
            "t",
            "l",
        );
        assert!(result.is_err());
    }

    #[test]
    fn duplicate_credential_id_fails() {
        let mgr = WebAuthnManager::new();
        mgr.begin_registration("alice");
        mgr.complete_registration("alice", "r", "cred-1", "pk", "yubikey", "l")
            .unwrap();

        mgr.begin_registration("alice");
        let result = mgr.complete_registration("alice", "r", "cred-1", "pk2", "yubikey", "l2");
        assert!(result.is_err());
    }

    #[test]
    fn begin_authentication_without_credentials_fails() {
        let mgr = WebAuthnManager::new();
        assert!(mgr.begin_authentication("alice").is_err());
    }

    #[test]
    fn full_auth_flow() {
        let mgr = WebAuthnManager::new();

        // تسجيل.
        mgr.begin_registration("alice");
        mgr.complete_registration("alice", "r", "cred-1", "pk", "yubikey", "مفتاح")
            .unwrap();

        // مصادقة.
        mgr.begin_authentication("alice").unwrap();
        mgr.complete_authentication("alice", "cred-1", "sig", 1)
            .unwrap();

        let creds = mgr.list_credentials("alice");
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].sign_count, 1);
        assert!(creds[0].last_used_at.is_some());
    }

    #[test]
    fn sign_count_replay_protection() {
        let mgr = WebAuthnManager::new();
        mgr.begin_registration("alice");
        mgr.complete_registration("alice", "r", "cred-1", "pk", "yubikey", "l")
            .unwrap();

        // مصادقة 1: sign_count = 5.
        mgr.begin_authentication("alice").unwrap();
        mgr.complete_authentication("alice", "cred-1", "sig", 5)
            .unwrap();

        // مصادقة 2: sign_count = 3 (أقل من 5) — يجب الرفض.
        mgr.begin_authentication("alice").unwrap();
        let result = mgr.complete_authentication("alice", "cred-1", "sig", 3);
        assert!(result.is_err());
    }

    #[test]
    fn empty_signature_rejected() {
        let mgr = WebAuthnManager::new();
        mgr.begin_registration("alice");
        mgr.complete_registration("alice", "r", "cred-1", "pk", "yubikey", "l")
            .unwrap();

        mgr.begin_authentication("alice").unwrap();
        let result = mgr.complete_authentication("alice", "cred-1", "", 1);
        assert!(result.is_err());
    }

    #[test]
    fn list_credentials() {
        let mgr = WebAuthnManager::new();
        mgr.begin_registration("alice");
        mgr.complete_registration("alice", "r", "c1", "pk1", "t1", "l1")
            .unwrap();
        mgr.begin_registration("alice");
        mgr.complete_registration("alice", "r", "c2", "pk2", "t2", "l2")
            .unwrap();

        let creds = mgr.list_credentials("alice");
        assert_eq!(creds.len(), 2);
    }

    #[test]
    fn remove_credential() {
        let mgr = WebAuthnManager::new();
        mgr.begin_registration("alice");
        mgr.complete_registration("alice", "r", "c1", "pk1", "t1", "l1")
            .unwrap();

        assert!(mgr.remove_credential("alice", "c1"));
        assert!(!mgr.is_registered("alice"));
    }

    #[test]
    fn remove_nonexistent_credential() {
        let mgr = WebAuthnManager::new();
        assert!(!mgr.remove_credential("alice", "nonexistent"));
    }

    #[test]
    fn registered_count() {
        let mgr = WebAuthnManager::new();
        assert_eq!(mgr.registered_count(), 0);

        mgr.begin_registration("alice");
        mgr.complete_registration("alice", "r", "c1", "pk", "t", "l")
            .unwrap();
        assert_eq!(mgr.registered_count(), 1);

        mgr.begin_registration("bob");
        mgr.complete_registration("bob", "r", "c2", "pk", "t", "l")
            .unwrap();
        assert_eq!(mgr.registered_count(), 2);
    }

    #[test]
    fn hash_data_deterministic() {
        let h1 = hash_data(b"test");
        let h2 = hash_data(b"test");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn hash_data_different_inputs() {
        let h1 = hash_data(b"hello");
        let h2 = hash_data(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn multiple_users_independent() {
        let mgr = WebAuthnManager::new();

        mgr.begin_registration("alice");
        mgr.complete_registration("alice", "r", "c1", "pk", "t", "l")
            .unwrap();

        mgr.begin_registration("bob");
        mgr.complete_registration("bob", "r", "c1", "pk", "t", "l")
            .unwrap();

        // كلاهما مسجّل.
        assert!(mgr.is_registered("alice"));
        assert!(mgr.is_registered("bob"));
    }

    #[test]
    fn cleanup_expired_challenges() {
        let mgr = WebAuthnManager::new();
        // أنشئ تحدياً وانتهِ صلاحيته يدوياً.
        let mut states = mgr.states.write();
        let state = states.entry("alice".into()).or_default();
        state.pending_challenges.push(Challenge {
            challenge: "old".into(),
            created_at: 0, // قديم جداً.
            used: false,
        });
        drop(states);

        let cleaned = mgr.cleanup_expired_challenges();
        assert_eq!(cleaned, 1);
    }
}
