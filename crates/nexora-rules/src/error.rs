//! أنواع أخطاء محرك القواعد.

use thiserror::Error;

pub type RuleResult<T> = Result<T, RuleError>;

#[derive(Debug, Error)]
pub enum RuleError {
    #[error("القاعدة غير موجودة: {0}")]
    NotFound(String),

    #[error("القاعدة غير مفعّلة: {0}")]
    NotEnabled(String),

    #[error("شرط غير صالح: {0}")]
    InvalidCondition(String),

    #[error("إجراء غير صالح: {0}")]
    InvalidAction(String),

    #[error("فشل تنفيذ الإجراء: {0}")]
    ActionFailed(String),

    #[error("فشل تقييم الشرط: {0}")]
    ConditionFailed(String),

    #[error("تعبير منطقي غير صالح: {0}")]
    InvalidExpression(String),

    #[error("خطأ في التسلسل: {0}")]
    Serde(String),

    #[error("القاعدة موجودة بالفعل: {0}")]
    AlreadyExists(String),
}

impl From<serde_json::Error> for RuleError {
    fn from(e: serde_json::Error) -> Self {
        RuleError::Serde(e.to_string())
    }
}
