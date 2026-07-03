//! # محرك الأمان Nexora
//!
//! كشف التهديدات والتنبيهات الأمنية. يحلل سجل التدقيق ويكتشف:
//! - محاولات تسجيل دخول فاشلة متكررة (brute force)
//! - دخول من مواقع IP مشبوهة
//! - نشاط غير معتاد (anomaly detection)
//! - تصعيد الصلاحيات المريب
//! - الوصول في أوقات غير معتادة
//!
//! # كيف يعمل
//!
//! 1. كل حدث تدقيق يُمرّر إلى `SecurityEngine::analyze()`
//! 2. المحرك يطبّق قواعد كشف مدمجة
//! 3. عند تجاوز عتبة، يُنشأ تنبيه أمني
//! 4. التنبيه يُسجّل في سجل التدقيق ويُرسل إشعار

pub mod alert;
pub mod detector;
pub mod engine;
pub mod threat;

pub use alert::{SecurityAlert, Severity, AlertStatus};
pub use detector::{BruteForceDetector, AnomalyDetector, Detector};
pub use engine::{SecurityEngine, SecurityStats};
pub use threat::{ThreatType, ThreatIndicator};
