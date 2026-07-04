//! إدارة MFA — ربط TOTP بحسابات المستخدمين.

use crate::mfa::totp::{generate_current_code, verify_code, TotpSecret};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use time::OffsetDateTime;

pub mod totp;

/// حالة MFA لمستخدم.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfaState {
    /// معرّف المستخدم.
    pub user_id: String,
    /// سر TOTP (إن وُجد، يعني أن MFA مُفعّل).
    pub secret: Option<TotpSecretData>,
    /// أكواد الاسترداد (للاستخدام عند فقدان الجهاز).
    pub backup_codes: Vec<String>,
    /// الأكواد المستخدمة (لمنع إعادة الاستخدام).
    pub used_backup_codes: HashSet<String>,
    /// هل MFA مُفعّل؟
    pub enabled: bool,
    /// وقت التفعيل.
    pub enabled_at: Option<i64>,
}

/// بيانات السر القابلة للتسلسل.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpSecretData {
    /// البايتات الخام مشفّرة hex.
    pub hex_bytes: String,
}

impl TotpSecretData {
    pub fn from_secret(secret: &TotpSecret) -> Self {
        Self {
            hex_bytes: hex::encode(&secret.bytes),
        }
    }

    pub fn to_secret(&self) -> Result<TotpSecret, String> {
        let bytes = hex::decode(&self.hex_bytes)
            .map_err(|e| format!("فشل فك تشفير hex: {e}"))?;
        Ok(TotpSecret::from_bytes(bytes))
    }
}

impl MfaState {
    /// إنشاء حالة فارغة (MFA غير مُفعّل).
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            secret: None,
            backup_codes: Vec::new(),
            used_backup_codes: HashSet::new(),
            enabled: false,
            enabled_at: None,
        }
    }

    /// هل MFA مُفعّل؟
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// مدير MFA.
pub struct MfaManager {
    /// حالات MFA لكل مستخدم.
    states: RwLock<HashMap<String, MfaState>>,
}

impl Default for MfaManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MfaManager {
    /// إنشاء مدير MFA جديد.
    pub fn new() -> Self {
        Self {
            states: RwLock::new(HashMap::new()),
        }
    }

    /// بدء تفعيل MFA لمستخدم — يُولّد سراً ويعيده (لم يُفعّل بعد).
    pub fn begin_enrollment(&self, user_id: &str) -> MfaEnrollment {
        let secret = TotpSecret::generate();
        MfaEnrollment {
            secret: secret.clone(),
            otpauth_url: secret.to_otpauth_url("Nexora", user_id),
            backup_codes: totp::generate_backup_codes(10),
        }
    }

    /// إكمال التفعيل — يتحقق من رمز صالح ثم يُفعّل MFA.
    pub fn complete_enrollment(
        &self,
        user_id: &str,
        enrollment: &MfaEnrollment,
        code: &str,
    ) -> Result<(), String> {
        if !verify_code(&enrollment.secret, code) {
            return Err("رمز التحقق غير صالح".into());
        }
        let mut states = self.states.write();
        let state = states.entry(user_id.to_string()).or_insert_with(|| MfaState::new(user_id));
        state.secret = Some(TotpSecretData::from_secret(&enrollment.secret));
        state.backup_codes = enrollment.backup_codes.clone();
        state.used_backup_codes = HashSet::new();
        state.enabled = true;
        state.enabled_at = Some(OffsetDateTime::now_utc().unix_timestamp_nanos() as i64);
        Ok(())
    }

    /// تعطيل MFA لمستخدم.
    pub fn disable(&self, user_id: &str) -> bool {
        let mut states = self.states.write();
        if let Some(state) = states.get_mut(user_id) {
            let was_enabled = state.enabled;
            state.enabled = false;
            state.secret = None;
            state.backup_codes.clear();
            state.used_backup_codes.clear();
            state.enabled_at = None;
            was_enabled
        } else {
            false
        }
    }

    /// التحقق من رمز MFA لمستخدم.
    pub fn verify(&self, user_id: &str, code: &str) -> MfaVerifyResult {
        let states = self.states.read();
        let state = match states.get(user_id) {
            Some(s) => s,
            None => return MfaVerifyResult::NotEnrolled,
        };
        if !state.enabled {
            return MfaVerifyResult::NotEnrolled;
        }
        let secret = match &state.secret {
            Some(s) => match s.to_secret() {
                Ok(s) => s,
                Err(_) => return MfaVerifyResult::Error("فشل تحميل السر".into()),
            },
            None => return MfaVerifyResult::NotEnrolled,
        };

        // تحقق من رمز TOTP أولاً.
        if verify_code(&secret, code) {
            return MfaVerifyResult::Valid;
        }

        // تحقق من أكواد الاسترداد.
        if state.backup_codes.contains(&code.to_string()) {
            // تحقق من عدم الاستخدام السابق.
            if state.used_backup_codes.contains(&code.to_string()) {
                return MfaVerifyResult::AlreadyUsed;
            }
            // سجّل الاستخدام.
            drop(states);
            let mut states = self.states.write();
            if let Some(s) = states.get_mut(user_id) {
                s.used_backup_codes.insert(code.to_string());
            }
            return MfaVerifyResult::Valid;
        }

        MfaVerifyResult::Invalid
    }

    /// هل المستخدم مُفعّل لديه MFA؟
    pub fn is_enrolled(&self, user_id: &str) -> bool {
        self.states
            .read()
            .get(user_id)
            .map(|s| s.enabled)
            .unwrap_or(false)
    }

    /// حالة MFA لمستخدم.
    pub fn get_state(&self, user_id: &str) -> Option<MfaState> {
        self.states.read().get(user_id).cloned()
    }

    /// عدد المستخدمين المُفعّلين.
    pub fn enrolled_count(&self) -> usize {
        self.states
            .read()
            .values()
            .filter(|s| s.enabled)
            .count()
    }

    /// توليد أكواد استرداد جديدة (تستبدل القديمة).
    pub fn regenerate_backup_codes(&self, user_id: &str) -> Result<Vec<String>, String> {
        let mut states = self.states.write();
        let state = states
            .get_mut(user_id)
            .ok_or_else(|| "المستخدم غير موجود".to_string())?;
        if !state.enabled {
            return Err("MFA غير مُفعّل".into());
        }
        let new_codes = totp::generate_backup_codes(10);
        state.backup_codes = new_codes.clone();
        state.used_backup_codes.clear();
        Ok(new_codes)
    }
}

/// نتيجة تسجيل MFA (يحتاجه المستخدم لإضافة إلى تطبيق المصادقة).
#[derive(Debug, Clone)]
pub struct MfaEnrollment {
    /// السر المشترك.
    pub secret: TotpSecret,
    /// URL بصيغة otpauth:// لتوليد QR code.
    pub otpauth_url: String,
    /// أكواد الاسترداد.
    pub backup_codes: Vec<String>,
}

/// نتيجة التحقق من رمز MFA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MfaVerifyResult {
    /// الرمز صالح.
    Valid,
    /// الرمز غير صالح.
    Invalid,
    /// المستخدم غير مُفعّل لديه MFA.
    NotEnrolled,
    /// كود الاسترداد مستخدم بالفعل.
    AlreadyUsed,
    /// خطأ في التحقق.
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_enrollment_generates_secret_and_url() {
        let mgr = MfaManager::new();
        let enrollment = mgr.begin_enrollment("alice");
        assert_eq!(enrollment.secret.bytes.len(), 20);
        assert!(enrollment.otpauth_url.starts_with("otpauth://totp/"));
        assert_eq!(enrollment.backup_codes.len(), 10);
    }

    #[test]
    fn complete_enrollment_with_valid_code() {
        let mgr = MfaManager::new();
        let enrollment = mgr.begin_enrollment("alice");
        let code = generate_current_code(&enrollment.secret);
        mgr.complete_enrollment("alice", &enrollment, &code).unwrap();
        assert!(mgr.is_enrolled("alice"));
    }

    #[test]
    fn complete_enrollment_with_invalid_code() {
        let mgr = MfaManager::new();
        let enrollment = mgr.begin_enrollment("alice");
        let result = mgr.complete_enrollment("alice", &enrollment, "000000");
        assert!(result.is_err());
        assert!(!mgr.is_enrolled("alice"));
    }

    #[test]
    fn verify_valid_totp_code() {
        let mgr = MfaManager::new();
        let enrollment = mgr.begin_enrollment("alice");
        let code = generate_current_code(&enrollment.secret);
        mgr.complete_enrollment("alice", &enrollment, &code).unwrap();

        // أنشئ كوداً جديداً (الكود القديم قد لا يزال صالحاً ضمن التسامح).
        let new_code = generate_current_code(&enrollment.secret);
        let result = mgr.verify("alice", &new_code);
        assert_eq!(result, MfaVerifyResult::Valid);
    }

    #[test]
    fn verify_invalid_code() {
        let mgr = MfaManager::new();
        let enrollment = mgr.begin_enrollment("alice");
        let code = generate_current_code(&enrollment.secret);
        mgr.complete_enrollment("alice", &enrollment, &code).unwrap();

        let result = mgr.verify("alice", "999999");
        assert_eq!(result, MfaVerifyResult::Invalid);
    }

    #[test]
    fn verify_for_unenrolled_user() {
        let mgr = MfaManager::new();
        let result = mgr.verify("bob", "123456");
        assert_eq!(result, MfaVerifyResult::NotEnrolled);
    }

    #[test]
    fn verify_backup_code() {
        let mgr = MfaManager::new();
        let enrollment = mgr.begin_enrollment("alice");
        let code = generate_current_code(&enrollment.secret);
        mgr.complete_enrollment("alice", &enrollment, &code).unwrap();

        let backup = enrollment.backup_codes[0].clone();
        let result = mgr.verify("alice", &backup);
        assert_eq!(result, MfaVerifyResult::Valid);
    }

    #[test]
    fn backup_code_cannot_be_reused() {
        let mgr = MfaManager::new();
        let enrollment = mgr.begin_enrollment("alice");
        let code = generate_current_code(&enrollment.secret);
        mgr.complete_enrollment("alice", &enrollment, &code).unwrap();

        let backup = enrollment.backup_codes[0].clone();
        assert_eq!(mgr.verify("alice", &backup), MfaVerifyResult::Valid);
        assert_eq!(mgr.verify("alice", &backup), MfaVerifyResult::AlreadyUsed);
    }

    #[test]
    fn disable_mfa() {
        let mgr = MfaManager::new();
        let enrollment = mgr.begin_enrollment("alice");
        let code = generate_current_code(&enrollment.secret);
        mgr.complete_enrollment("alice", &enrollment, &code).unwrap();
        assert!(mgr.is_enrolled("alice"));

        assert!(mgr.disable("alice"));
        assert!(!mgr.is_enrolled("alice"));
    }

    #[test]
    fn disable_nonexistent_returns_false() {
        let mgr = MfaManager::new();
        assert!(!mgr.disable("nobody"));
    }

    #[test]
    fn enrolled_count() {
        let mgr = MfaManager::new();
        assert_eq!(mgr.enrolled_count(), 0);

        let e1 = mgr.begin_enrollment("alice");
        let c1 = generate_current_code(&e1.secret);
        mgr.complete_enrollment("alice", &e1, &c1).unwrap();
        assert_eq!(mgr.enrolled_count(), 1);

        let e2 = mgr.begin_enrollment("bob");
        let c2 = generate_current_code(&e2.secret);
        mgr.complete_enrollment("bob", &e2, &c2).unwrap();
        assert_eq!(mgr.enrolled_count(), 2);

        mgr.disable("alice");
        assert_eq!(mgr.enrolled_count(), 1);
    }

    #[test]
    fn regenerate_backup_codes() {
        let mgr = MfaManager::new();
        let enrollment = mgr.begin_enrollment("alice");
        let code = generate_current_code(&enrollment.secret);
        mgr.complete_enrollment("alice", &enrollment, &code).unwrap();

        let old_codes = enrollment.backup_codes.clone();
        let new_codes = mgr.regenerate_backup_codes("alice").unwrap();
        assert_eq!(new_codes.len(), 10);
        // الأكواد الجديدة يجب أن تختلف عن القديمة (احتمال التطابق منخفض).
        assert_ne!(old_codes, new_codes);

        // الكود القديم يجب ألا يعمل.
        let old_backup = old_codes[0].clone();
        assert_eq!(mgr.verify("alice", &old_backup), MfaVerifyResult::Invalid);

        // الكود الجديد يجب أن يعمل.
        let new_backup = new_codes[0].clone();
        assert_eq!(mgr.verify("alice", &new_backup), MfaVerifyResult::Valid);
    }

    #[test]
    fn regenerate_for_nonexistent_fails() {
        let mgr = MfaManager::new();
        assert!(mgr.regenerate_backup_codes("nobody").is_err());
    }

    #[test]
    fn get_state_returns_enabled_state() {
        let mgr = MfaManager::new();
        let enrollment = mgr.begin_enrollment("alice");
        let code = generate_current_code(&enrollment.secret);
        mgr.complete_enrollment("alice", &enrollment, &code).unwrap();

        let state = mgr.get_state("alice").unwrap();
        assert!(state.enabled);
        assert!(state.secret.is_some());
        assert_eq!(state.backup_codes.len(), 10);
        assert!(state.enabled_at.is_some());
    }

    #[test]
    fn totp_secret_data_roundtrip() {
        let secret = TotpSecret::generate();
        let data = TotpSecretData::from_secret(&secret);
        let recovered = data.to_secret().unwrap();
        assert_eq!(secret.bytes, recovered.bytes);
    }
}
