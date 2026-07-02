//! # مصادقة متعددة العوامل (MFA) عبر TOTP
//!
//! تنفيذ RFC 6238 — كلمات مرور لمرة واحدة مبنية على الوقت (TOTP).
//!
//! # كيف يعمل
//!
//! 1. المستخدم يُفعّل MFA — يُولّد سر مشترك (shared secret)
//! 2. السر يُعرض كـ QR code (otpauth:// URL) للمستخدم
//! 3. المستخدم يضيفه إلى تطبيق مصادقة (Google Authenticator, Authy)
//! 4. عند تسجيل الدخول، المستخدم يدخل رمز 6 أرقام
//! 5. الخادم يتحقق من الرمز باستخدام نفس السر + الوقت الحالي
//!
//! # الخوارزمية
//!
//! TOTP = HOTP(secret, floor(unix_time / 30))
//! HOTP = HMAC-SHA1(secret, counter) → آخر 4 بايت → mod 10^6

use hmac::{Hmac, Mac};
use sha1::Sha1;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha1 = Hmac<Sha1>;

/// طول الرمز (6 أرقام افتراضياً).
const DIGITS: u32 = 6;
/// فترة الرمز بالثواني (30 ثانية قياسية).
const PERIOD: u64 = 30;
/// عدد الخطوات المسموح بها للتباين (±1 خطوة = ±30 ثانية).
const TOLERANCE: u64 = 1;

/// سر TOTP المشترك (20 بايت افتراضياً، base32 encoded).
#[derive(Debug, Clone)]
pub struct TotpSecret {
    /// البايتات الخام للسر.
    pub bytes: Vec<u8>,
}

impl TotpSecret {
    /// توليد سر عشوائي جديد (20 بايت).
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut bytes = vec![0u8; 20];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self { bytes }
    }

    /// إنشاء سر من بايتات موجودة.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// ترميز السر بصيغة Base32 (للاستخدام في QR codes).
    pub fn to_base32(&self) -> String {
        base32_encode(&self.bytes)
    }

    /// فك ترميز سر من Base32.
    pub fn from_base32(s: &str) -> Result<Self, String> {
        let bytes = base32_decode(s)?;
        Ok(Self { bytes })
    }

    /// بناء URL بصيغة otpauth:// لتوليد QR code.
    pub fn to_otpauth_url(&self, issuer: &str, account: &str) -> String {
        let secret = self.to_base32();
        let issuer_encoded = url_encode(issuer);
        let account_encoded = url_encode(account);
        format!(
            "otpauth://totp/{issuer_encoded}:{account_encoded}?secret={secret}&issuer={issuer_encoded}&digits={DIGITS}&period={PERIOD}"
        )
    }
}

/// حساب الرمز الحالي لسر معين.
pub fn generate_current_code(secret: &TotpSecret) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let counter = now / PERIOD;
    generate_code_at(secret, counter)
}

/// حساب الرمز عند خطوة زمنية محددة.
pub fn generate_code_at(secret: &TotpSecret, counter: u64) -> String {
    let mut mac = HmacSha1::new_from_slice(&secret.bytes).expect("HMAC accepts any key length");
    let counter_bytes = counter.to_be_bytes();
    mac.update(&counter_bytes);
    let result = mac.finalize().into_bytes();

    // Dynamic truncation: آخر 4 بايت
    let offset = (result[19] & 0x0f) as usize;
    let truncated: u32 = ((result[offset] as u32 & 0x7f) << 24)
        | ((result[offset + 1] as u32) << 16)
        | ((result[offset + 2] as u32) << 8)
        | (result[offset + 3] as u32);

    let code = truncated % 10u32.pow(DIGITS);
    format!("{code:06}")
}

/// التحقق من رمز TOTP مع سماحية تباين ±1 خطوة زمنية.
pub fn verify_code(secret: &TotpSecret, code: &str) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let current_counter = now / PERIOD;

    // تحقق من النطاق ±TOLERANCE خطوة.
    for offset in 0..=TOLERANCE {
        // الحالي
        if let Some(c) = current_counter.checked_sub(offset) {
            if generate_code_at(secret, c) == code {
                return true;
            }
        }
        // المستقبلي (offset > 0 فقط)
        if offset > 0 {
            let future = current_counter + offset;
            if generate_code_at(secret, future) == code {
                return true;
            }
        }
    }
    false
}

/// توليد أكواد استرداد (backup codes) للاستخدام عند فقدان الجهاز.
pub fn generate_backup_codes(count: usize) -> Vec<String> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..count)
        .map(|_| {
            let n: u64 = rng.gen_range(0..1_0000_0000_0000);
            format!("{n:012}")
        })
        .collect()
}

/// ترميز Base32 (RFC 4648) — بدون padding.
fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut result = String::with_capacity((data.len() * 8 + 4) / 5);
    let mut buffer: u64 = 0;
    let mut bits = 0;
    for &byte in data {
        buffer = (buffer << 8) | byte as u64;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buffer >> bits) & 0x1f) as usize;
            result.push(ALPHABET[idx] as char);
        }
    }
    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 0x1f) as usize;
        result.push(ALPHABET[idx] as char);
    }
    result
}

/// فك ترميز Base32.
fn base32_decode(s: &str) -> Result<Vec<u8>, String> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let s = s.to_uppercase();
    let s: String = s.chars().filter(|c| !c.is_whitespace() && *c != '=').collect();
    let mut buffer: u64 = 0;
    let mut bits = 0;
    let mut result = Vec::new();
    for c in s.chars() {
        let idx = ALPHABET.iter().position(|&a| a as char == c)
            .ok_or_else(|| format!("حرف Base32 غير صالح: {c}"))?;
        buffer = (buffer << 5) | idx as u64;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            result.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Ok(result)
}

/// ترميز URL بسيط.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
            out.push(c);
        } else {
            for b in c.to_string().bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_generate_is_20_bytes() {
        let s = TotpSecret::generate();
        assert_eq!(s.bytes.len(), 20);
    }

    #[test]
    fn secret_base32_roundtrip() {
        let s = TotpSecret::generate();
        let encoded = s.to_base32();
        let decoded = TotpSecret::from_base32(&encoded).unwrap();
        assert_eq!(s.bytes, decoded.bytes);
    }

    #[test]
    fn base32_encode_known_value() {
        // "Hello" in Base32 = "JBSWY3DP"
        let encoded = base32_encode(b"Hello");
        assert_eq!(encoded, "JBSWY3DP");
    }

    #[test]
    fn base32_decode_known_value() {
        let decoded = base32_decode("JBSWY3DP").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn base32_decode_invalid_char() {
        assert!(base32_decode("INVALID!@#").is_err());
    }

    #[test]
    fn base32_decode_lowercase_works() {
        let decoded = base32_decode("jbswy3dp").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn code_is_6_digits() {
        let s = TotpSecret::generate();
        let code = generate_current_code(&s);
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn code_at_known_counter() {
        // RFC 6238 test vector: secret = "12345678901234567890", counter = 1
        let secret = TotpSecret::from_bytes(b"12345678901234567890".to_vec());
        let code = generate_code_at(&secret, 1);
        assert_eq!(code.len(), 6);
        // HOTP with counter=1 for this secret should be 287082
        assert_eq!(code, "287082");
    }

    #[test]
    fn code_at_counter_2() {
        let secret = TotpSecret::from_bytes(b"12345678901234567890".to_vec());
        let code = generate_code_at(&secret, 2);
        assert_eq!(code, "359152");
    }

    #[test]
    fn verify_current_code() {
        let s = TotpSecret::generate();
        let code = generate_current_code(&s);
        assert!(verify_code(&s, &code));
    }

    #[test]
    fn verify_wrong_code() {
        let s = TotpSecret::generate();
        assert!(!verify_code(&s, "000000"));
    }

    #[test]
    fn verify_empty_code() {
        let s = TotpSecret::generate();
        assert!(!verify_code(&s, ""));
    }

    #[test]
    fn verify_non_digit_code() {
        let s = TotpSecret::generate();
        assert!(!verify_code(&s, "abcdef"));
    }

    #[test]
    fn backup_codes_count() {
        let codes = generate_backup_codes(10);
        assert_eq!(codes.len(), 10);
        for c in &codes {
            assert_eq!(c.len(), 12);
            assert!(c.chars().all(|ch| ch.is_ascii_digit()));
        }
    }

    #[test]
    fn backup_codes_are_unique() {
        let codes = generate_backup_codes(100);
        let unique: std::collections::HashSet<_> = codes.iter().collect();
        // احتمال التضارب منخفض جداً، يجب أن يكون كلها فريدة
        assert!(unique.len() > 95);
    }

    #[test]
    fn otpauth_url_format() {
        let s = TotpSecret::from_bytes(b"12345678901234567890".to_vec());
        let url = s.to_otpauth_url("Nexora", "alice@example.com");
        assert!(url.starts_with("otpauth://totp/"));
        assert!(url.contains("secret="));
        assert!(url.contains("issuer=Nexora"));
        assert!(url.contains("digits=6"));
        assert!(url.contains("period=30"));
    }

    #[test]
    fn otpauth_url_encodes_special_chars() {
        let s = TotpSecret::generate();
        let url = s.to_otpauth_url("My App+Co", "user@test.com");
        assert!(url.contains("My%20App%2BCo"));
        assert!(url.contains("user%40test.com"));
    }

    #[test]
    fn code_changes_over_time() {
        let s = TotpSecret::generate();
        let code1 = generate_code_at(&s, 0);
        let code2 = generate_code_at(&s, 1);
        let code3 = generate_code_at(&s, 2);
        // الأكواد يجب أن تختلف (احتمال التطابق منخفض جداً)
        assert_ne!(code1, code2);
        assert_ne!(code2, code3);
        assert_ne!(code1, code3);
    }

    #[test]
    fn same_secret_same_code() {
        let s = TotpSecret::from_bytes(b"test_secret_20_bytes!".to_vec());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let counter = now / PERIOD;
        let code1 = generate_code_at(&s, counter);
        let code2 = generate_code_at(&s, counter);
        assert_eq!(code1, code2);
    }

    #[test]
    fn verify_with_tolerance_past() {
        let s = TotpSecret::generate();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // كود من خطوة سابقة (ضمن التسامح)
        let past_counter = now / PERIOD - 1;
        let past_code = generate_code_at(&s, past_counter);
        assert!(verify_code(&s, &past_code));
    }

    #[test]
    fn verify_with_tolerance_future() {
        let s = TotpSecret::generate();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // كود من خطوة مستقبلية (ضمن التسامح)
        let future_counter = now / PERIOD + 1;
        let future_code = generate_code_at(&s, future_counter);
        assert!(verify_code(&s, &future_code));
    }

    #[test]
    fn verify_outside_tolerance_fails() {
        let s = TotpSecret::generate();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // كود من خطوة بعيدة جداً (خارج التسامح)
        let far_counter = now / PERIOD + 5;
        let far_code = generate_code_at(&s, far_counter);
        assert!(!verify_code(&s, &far_code));
    }

    // إضافة sha1 كتبعية للاختبار
}
