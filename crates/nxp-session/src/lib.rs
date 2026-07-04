//! طبقة جلسة NXP.
//!
//! انظر RFC §2.2. تُنشأ الجلسة بأمر `HELLO` فوراً بعد مصافحة QUIC.
//! هذه الوحدة توفر بروتوكول المصافحة، حالة الجلسة، ومنطق نبض القلب.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod handshake;
pub mod manager;
pub mod time;

pub use handshake::{HelloPayload, HelloAckPayload, HandshakeError};
pub use manager::{Session, SessionManager, SessionState};
pub use time::{now_us, skew_ok, MAX_SKEW_US};
