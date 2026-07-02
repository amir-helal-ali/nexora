//! طبقة نقل NXP.
//!
//! انظر RFC §2.1. يعمل NXP فوق QUIC، الذي يوفر TLS 1.3، تدفقات متعددة،
//! استئناف 0-RTT، وهجرة الاتصال خارج الصندوق.
//! هذه الـ crate تغلّف تطبيق `quinn` QUIC وتوفر:
//! - `NxpServer` — يقبل اتصالات NXP الواردة
//! - `NxpClient` — ينشئ جلسات NXP الصادرة
//! - `NxpConnection` — قراءة/كتابة على مستوى الإطار عبر تدفق QUIC
//!
//! طبقة TLS مُهيأة لـ **شهادات ذاتية التوقيع افتراضياً**،
//! وهو مناسب للتواصل الداخلي للعنقود حيث تُنشأ الهوية في طبقة جلسة NXP
//! (مفاتيح هوية Ed25519). للدخول الخارجي، تنهي بوابة API TLS بشهادات
//! موثوقة عامة.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod cert;
pub mod conn;
pub mod server;
pub mod client;

pub use client::NxpClient;
pub use conn::{NxpConnection, ReadFrameError};
pub use server::NxpServer;
