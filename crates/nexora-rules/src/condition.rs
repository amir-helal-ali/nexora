//! الشروط (Conditions) — تحدد متى تُنفّذ القاعدة.

use crate::error::{RuleError, RuleResult};
use nexora_core::events::Event;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// نتيجة تقييم الشرط.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionResult {
    /// الشرط محقَّق — نفّذ الإجراء.
    Matched,
    /// الشرط غير محقَّق — تخطَّ القاعدة.
    NotMatched,
}

/// نوع الشرط.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConditionKind {
    /// يطابق اسم الحدث نمط glob (مثلاً `billing.*`).
    EventNameMatches {
        pattern: String,
    },
    /// يطابق اسم الحدث تعبيراً منتظماً.
    EventNameRegex {
        pattern: String,
    },
    /// حمولة الحدث (نصية) تحتوي على سلسلة محددة.
    PayloadContains {
        substring: String,
    },
    /// حمولة الحدث تطابق تعبيراً منتظماً.
    PayloadRegex {
        pattern: String,
    },
    /// حمولة الحدث تساوي قيمة محددة تماماً.
    PayloadEquals {
        value: String,
    },
    /// شرط "أي حدث" — دائماً محقَّق.
    Always,
    /// شرط "أبداً" — دائماً غير محقَّق.
    Never,
    /// شرط مركب: كل الشروط الفرعية يجب أن تتحقق (AND).
    All {
        conditions: Vec<Condition>,
    },
    /// شرط مركب: شرط فرعي واحد على الأقل يجب أن يتحقق (OR).
    Any {
        conditions: Vec<Condition>,
    },
    /// شرط مركب: عكس الشرط الفرعي (NOT).
    Not {
        condition: Box<Condition>,
    },
}

/// شرط قابل للتقييم.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    /// نوع الشرط ومعامله.
    pub kind: ConditionKind,
}

impl Condition {
    /// إنشاء شرط "دائماً".
    pub fn always() -> Self {
        Self {
            kind: ConditionKind::Always,
        }
    }

    /// إنشاء شرط "أبداً".
    pub fn never() -> Self {
        Self {
            kind: ConditionKind::Never,
        }
    }

    /// إنشاء شرط مطابقة اسم حدث بنمط glob.
    pub fn event_name_matches(pattern: impl Into<String>) -> Self {
        Self {
            kind: ConditionKind::EventNameMatches {
                pattern: pattern.into(),
            },
        }
    }

    /// إنشاء شرط مطابقة اسم حدث بتعبير منتظم.
    pub fn event_name_regex(pattern: impl Into<String>) -> Self {
        Self {
            kind: ConditionKind::EventNameRegex {
                pattern: pattern.into(),
            },
        }
    }

    /// إنشاء شرط احتواء الحمولة.
    pub fn payload_contains(substring: impl Into<String>) -> Self {
        Self {
            kind: ConditionKind::PayloadContains {
                substring: substring.into(),
            },
        }
    }

    /// إنشاء شرط AND.
    pub fn all(conditions: Vec<Condition>) -> Self {
        Self {
            kind: ConditionKind::All { conditions },
        }
    }

    /// إنشاء شرط OR.
    pub fn any(conditions: Vec<Condition>) -> Self {
        Self {
            kind: ConditionKind::Any { conditions },
        }
    }

    /// إنشاء شرط NOT.
    pub fn not(condition: Condition) -> Self {
        Self {
            kind: ConditionKind::Not {
                condition: Box::new(condition),
            },
        }
    }

    /// تقييم الشرط ضد حدث.
    pub fn evaluate(&self, event: &Event) -> RuleResult<ConditionResult> {
        let matched = self.evaluate_bool(event)?;
        Ok(if matched {
            ConditionResult::Matched
        } else {
            ConditionResult::NotMatched
        })
    }

    fn evaluate_bool(&self, event: &Event) -> RuleResult<bool> {
        match &self.kind {
            ConditionKind::Always => Ok(true),
            ConditionKind::Never => Ok(false),
            ConditionKind::EventNameMatches { pattern } => {
                Ok(glob_match(pattern, &event.name))
            }
            ConditionKind::EventNameRegex { pattern } => {
                let re = Regex::new(pattern)
                    .map_err(|e| RuleError::InvalidExpression(e.to_string()))?;
                Ok(re.is_match(&event.name))
            }
            ConditionKind::PayloadContains { substring } => {
                let payload_str = payload_to_string(&event.payload);
                Ok(payload_str.contains(substring))
            }
            ConditionKind::PayloadRegex { pattern } => {
                let re = Regex::new(pattern)
                    .map_err(|e| RuleError::InvalidExpression(e.to_string()))?;
                let payload_str = payload_to_string(&event.payload);
                Ok(re.is_match(&payload_str))
            }
            ConditionKind::PayloadEquals { value } => {
                let payload_str = payload_to_string(&event.payload);
                Ok(payload_str == *value)
            }
            ConditionKind::All { conditions } => {
                for c in conditions {
                    if !c.evaluate_bool(event)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            ConditionKind::Any { conditions } => {
                for c in conditions {
                    if c.evaluate_bool(event)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            ConditionKind::Not { condition } => {
                Ok(!condition.evaluate_bool(event)?)
            }
        }
    }
}

/// تحويل حمولة الحدث إلى سلسلة نصية للتقييم.
fn payload_to_string(payload: &nexora_core::events::EventPayload) -> String {
    match payload {
        nexora_core::events::EventPayload::Text(s) => s.clone(),
        nexora_core::events::EventPayload::Bytes(b) => {
            String::from_utf8_lossy(b).to_string()
        }
        nexora_core::events::EventPayload::Empty => String::new(),
    }
}

/// مطابقة نمط glob بسيط (يدعم `*` فقط).
fn glob_match(pattern: &str, text: &str) -> bool {
    // تحويل glob إلى regex بسيط: `*` ← `.*`
    let mut regex_str = String::with_capacity(pattern.len() * 2);
    regex_str.push('^');
    for c in pattern.chars() {
        match c {
            '*' => regex_str.push_str(".*"),
            '?' => regex_str.push('.'),
            c if "\\^$.|+()[]{}".contains(c) => {
                regex_str.push('\\');
                regex_str.push(c);
            }
            c => regex_str.push(c),
        }
    }
    regex_str.push('$');
    match Regex::new(&regex_str) {
        Ok(re) => re.is_match(text),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_core::events::{Event, EventId, EventPayload};

    fn make_event(name: &str, payload: &str) -> Event {
        Event {
            id: 1,
            name: name.to_string(),
            payload: EventPayload::Text(payload.to_string()),
            timestamp: 0,
        }
    }

    #[test]
    fn always_matches() {
        let c = Condition::always();
        let e = make_event("anything", "anything");
        assert_eq!(c.evaluate(&e).unwrap(), ConditionResult::Matched);
    }

    #[test]
    fn never_matches() {
        let c = Condition::never();
        let e = make_event("anything", "anything");
        assert_eq!(c.evaluate(&e).unwrap(), ConditionResult::NotMatched);
    }

    #[test]
    fn event_name_glob_match() {
        let c = Condition::event_name_matches("billing.*");
        assert_eq!(
            c.evaluate(&make_event("billing.invoice.created", "")).unwrap(),
            ConditionResult::Matched
        );
        assert_eq!(
            c.evaluate(&make_event("auth.login", "")).unwrap(),
            ConditionResult::NotMatched
        );
    }

    #[test]
    fn event_name_regex_match() {
        let c = Condition::event_name_regex(r"^user\.(created|deleted)$");
        assert_eq!(
            c.evaluate(&make_event("user.created", "")).unwrap(),
            ConditionResult::Matched
        );
        assert_eq!(
            c.evaluate(&make_event("user.updated", "")).unwrap(),
            ConditionResult::NotMatched
        );
    }

    #[test]
    fn payload_contains() {
        let c = Condition::payload_contains("failed");
        assert_eq!(
            c.evaluate(&make_event("x", "payment failed")).unwrap(),
            ConditionResult::Matched
        );
        assert_eq!(
            c.evaluate(&make_event("x", "payment succeeded")).unwrap(),
            ConditionResult::NotMatched
        );
    }

    #[test]
    fn payload_equals() {
        let c = Condition {
            kind: ConditionKind::PayloadEquals {
                value: "exact".into(),
            },
        };
        assert_eq!(
            c.evaluate(&make_event("x", "exact")).unwrap(),
            ConditionResult::Matched
        );
        assert_eq!(
            c.evaluate(&make_event("x", "not exact")).unwrap(),
            ConditionResult::NotMatched
        );
    }

    #[test]
    fn payload_regex() {
        let c = Condition {
            kind: ConditionKind::PayloadRegex {
                pattern: r"\d{3,}".into(),
            },
        };
        assert_eq!(
            c.evaluate(&make_event("x", "order 12345")).unwrap(),
            ConditionResult::Matched
        );
        assert_eq!(
            c.evaluate(&make_event("x", "no numbers")).unwrap(),
            ConditionResult::NotMatched
        );
    }

    #[test]
    fn all_conditions_and() {
        let c = Condition::all(vec![
            Condition::event_name_matches("billing.*"),
            Condition::payload_contains("failed"),
        ]);
        assert_eq!(
            c.evaluate(&make_event("billing.payment", "status: failed")).unwrap(),
            ConditionResult::Matched
        );
        assert_eq!(
            c.evaluate(&make_event("billing.payment", "status: ok")).unwrap(),
            ConditionResult::NotMatched
        );
        assert_eq!(
            c.evaluate(&make_event("auth.login", "failed")).unwrap(),
            ConditionResult::NotMatched
        );
    }

    #[test]
    fn any_conditions_or() {
        let c = Condition::any(vec![
            Condition::event_name_matches("billing.*"),
            Condition::payload_contains("urgent"),
        ]);
        assert_eq!(
            c.evaluate(&make_event("billing.x", "")).unwrap(),
            ConditionResult::Matched
        );
        assert_eq!(
            c.evaluate(&make_event("other", "urgent message")).unwrap(),
            ConditionResult::Matched
        );
        assert_eq!(
            c.evaluate(&make_event("other", "normal")).unwrap(),
            ConditionResult::NotMatched
        );
    }

    #[test]
    fn not_condition() {
        let c = Condition::not(Condition::payload_contains("failed"));
        assert_eq!(
            c.evaluate(&make_event("x", "success")).unwrap(),
            ConditionResult::Matched
        );
        assert_eq!(
            c.evaluate(&make_event("x", "failed")).unwrap(),
            ConditionResult::NotMatched
        );
    }

    #[test]
    fn nested_conditions() {
        // (billing.* AND contains:failed) OR (auth.* AND contains:denied)
        let c = Condition::any(vec![
            Condition::all(vec![
                Condition::event_name_matches("billing.*"),
                Condition::payload_contains("failed"),
            ]),
            Condition::all(vec![
                Condition::event_name_matches("auth.*"),
                Condition::payload_contains("denied"),
            ]),
        ]);
        assert_eq!(
            c.evaluate(&make_event("billing.payment", "failed")).unwrap(),
            ConditionResult::Matched
        );
        assert_eq!(
            c.evaluate(&make_event("auth.login", "access denied")).unwrap(),
            ConditionResult::Matched
        );
        assert_eq!(
            c.evaluate(&make_event("other.event", "ok")).unwrap(),
            ConditionResult::NotMatched
        );
    }

    #[test]
    fn empty_payload_contains() {
        let c = Condition::payload_contains("x");
        let e = Event {
            id: 1,
            name: "test".into(),
            payload: EventPayload::Empty,
            timestamp: 0,
        };
        assert_eq!(c.evaluate(&e).unwrap(), ConditionResult::NotMatched);
    }

    #[test]
    fn invalid_regex_returns_error() {
        let c = Condition::event_name_regex("[invalid");
        let e = make_event("test", "");
        assert!(c.evaluate(&e).is_err());
    }

    #[test]
    fn glob_match_question_mark() {
        let c = Condition::event_name_matches("user.?reated");
        assert_eq!(
            c.evaluate(&make_event("user.created", "")).unwrap(),
            ConditionResult::Matched
        );
        assert_eq!(
            c.evaluate(&make_event("user.deleted", "")).unwrap(),
            ConditionResult::NotMatched
        );
    }

    #[test]
    fn serde_roundtrip() {
        let c = Condition::all(vec![
            Condition::event_name_matches("test.*"),
            Condition::payload_contains("hello"),
        ]);
        let json = serde_json::to_string(&c).unwrap();
        let back: Condition = serde_json::from_str(&json).unwrap();
        let e = make_event("test.event", "hello world");
        assert_eq!(back.evaluate(&e).unwrap(), ConditionResult::Matched);
    }
}
