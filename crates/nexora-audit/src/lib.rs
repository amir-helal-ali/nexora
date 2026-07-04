//! # سجل تدقيق Nexora
//!
//! تتبع شامل لكل النشاطات في المنصة. كل إجراء حساس يُسجّل مع:
//! - من فعله (actor)
//! - ماذا فعل (action)
//! - على ما (target)
//! - متى (timestamp)
//! - سياق إضافي (metadata)
//!
//! # الاستخدام
//!
//! ```no_run
//! # fn main() {
//! use nexora_audit::{AuditLogger, AuditEntry, AuditCategory};
//!
//! let logger = AuditLogger::new(100_000);
//! logger.log(AuditEntry::new("user-1", "login", "session-123")
//!     .with_category(AuditCategory::Auth)
//!     .with_metadata("ip", "192.168.1.1"));
//! # }
//! ```

pub mod entry;
pub mod filter;
pub mod logger;
pub mod category;

pub use category::AuditCategory;
pub use entry::{AuditEntry, AuditEntryId};
pub use filter::{AuditFilter, AuditSort};
pub use logger::{AuditLogger, AuditQueryResult};
