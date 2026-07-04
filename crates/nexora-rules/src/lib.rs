//! # محرك قواعد Nexora
//!
//! محرك أتمتة يحركه الأحداث (على نمط Zapier/IFTTT). يتيح للمستخدمين
//! تعريف قواعد من نوع "عندما يحدث X، نفّذ Y".
//!
//! # المفاهيم
//!
//! - **القاعدة (Rule)**: تعريف 条件 + إجراء
//! - **المُشغِّل (Trigger)**: حدث على ناقل الأحداث يبدأ القاعدة
//! - **الشرط (Condition)**: تعبير منطقي يحدد متى تُنفّذ القاعدة
//! - **الإجراء (Action)**: ما يتم تنفيذه (إرسال إشعار، نشر حدث، استدعاء webhook)
//!
//! # مثال
//!
//! ```text
//! القاعدة: "تنبيه الفشل"
//!   المُشغِّل: event.name matches "billing.*"
//!   الشرط: event.payload contains "failed"
//!   الإجراء: إرسال إشعار Slack إلى قناة #alerts
//! ```

pub mod action;
pub mod condition;
pub mod error;
pub mod rule;
pub mod engine;

pub use action::{Action, ActionKind, ActionResult};
pub use condition::{Condition, ConditionKind, ConditionResult};
pub use engine::{RuleEngine, RuleEngineStats};
pub use error::{RuleError, RuleResult};
pub use rule::{Rule, RuleId, RuleStatus};

