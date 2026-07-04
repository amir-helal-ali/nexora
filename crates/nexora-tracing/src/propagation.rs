//! Propagation — نقل سياق التتبع بين الخدمات.
//!
//! عبر ترويسات HTTP:
//! - `X-Trace-Id`: معرّف التتبع
//! - `X-Span-Id`: معرّف السلسلة الحالي
//! - `X-Parent-Span-Id`: معرّف السلسلة الأب

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// معرّف التتبع (32 بايت hex).
pub type TraceId = String;

/// توليد معرّف تتبع فريد (32 بايت hex).
pub fn generate_trace_id() -> TraceId {
    uuid::Uuid::new_v4().to_string().replace('-', "")
}

/// سياق Span — يُنقل بين الخدمات.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanContext {
    /// معرّف التتبع.
    pub trace_id: TraceId,
    /// معرّف Span الأب (إن وُجد).
    pub parent_span_id: Option<String>,
}

impl SpanContext {
    /// توليد سياق جديد (root).
    pub fn new_root() -> Self {
        Self {
            trace_id: generate_trace_id(),
            parent_span_id: None,
        }
    }

    /// إنشاء سياق فرعي.
    pub fn new_child(trace_id: TraceId, parent_span_id: String) -> Self {
        Self {
            trace_id,
            parent_span_id: Some(parent_span_id),
        }
    }

    /// استخراج السياق من ترويسات HTTP.
    pub fn from_headers(headers: &HashMap<String, String>) -> Option<Self> {
        let trace_id = headers.get("x-trace-id")?.clone();
        let parent_span_id = headers.get("x-parent-span-id").cloned();
        Some(Self {
            trace_id,
            parent_span_id,
        })
    }

    /// حقن السياق في ترويسات HTTP.
    pub fn to_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        headers.insert("x-trace-id".into(), self.trace_id.clone());
        if let Some(ref parent) = self.parent_span_id {
            headers.insert("x-parent-span-id".into(), parent.clone());
        }
        headers
    }
}

/// بناء ترويسات HTTP من سياق.
pub fn propagate_headers(context: &SpanContext) -> HashMap<String, String> {
    context.to_headers()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_id_is_32_hex() {
        let id = generate_trace_id();
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn trace_id_unique() {
        let id1 = generate_trace_id();
        let id2 = generate_trace_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn new_root_has_no_parent() {
        let ctx = SpanContext::new_root();
        assert!(ctx.parent_span_id.is_none());
        assert!(!ctx.trace_id.is_empty());
    }

    #[test]
    fn new_child_has_parent() {
        let trace_id = generate_trace_id();
        let ctx = SpanContext::new_child(trace_id.clone(), "parent-123".into());
        assert_eq!(ctx.trace_id, trace_id);
        assert_eq!(ctx.parent_span_id, Some("parent-123".into()));
    }

    #[test]
    fn to_headers_includes_trace_id() {
        let ctx = SpanContext::new_root();
        let headers = ctx.to_headers();
        assert!(headers.contains_key("x-trace-id"));
        assert!(!headers.contains_key("x-parent-span-id"));
    }

    #[test]
    fn to_headers_includes_parent() {
        let ctx = SpanContext::new_child(generate_trace_id(), "parent-1".into());
        let headers = ctx.to_headers();
        assert!(headers.contains_key("x-trace-id"));
        assert!(headers.contains_key("x-parent-span-id"));
    }

    #[test]
    fn from_headers_extracts_context() {
        let mut headers = HashMap::new();
        headers.insert("x-trace-id".into(), "abc123".into());
        headers.insert("x-parent-span-id".into(), "parent-456".into());

        let ctx = SpanContext::from_headers(&headers).unwrap();
        assert_eq!(ctx.trace_id, "abc123");
        assert_eq!(ctx.parent_span_id, Some("parent-456".into()));
    }

    #[test]
    fn from_headers_without_trace_id_returns_none() {
        let headers = HashMap::new();
        assert!(SpanContext::from_headers(&headers).is_none());
    }

    #[test]
    fn from_headers_without_parent() {
        let mut headers = HashMap::new();
        headers.insert("x-trace-id".into(), "trace-1".into());

        let ctx = SpanContext::from_headers(&headers).unwrap();
        assert_eq!(ctx.trace_id, "trace-1");
        assert!(ctx.parent_span_id.is_none());
    }

    #[test]
    fn roundtrip_headers() {
        let ctx = SpanContext::new_child(generate_trace_id(), "parent-abc".into());
        let headers = ctx.to_headers();
        let recovered = SpanContext::from_headers(&headers).unwrap();
        assert_eq!(ctx.trace_id, recovered.trace_id);
        assert_eq!(ctx.parent_span_id, recovered.parent_span_id);
    }

    #[test]
    fn propagate_headers_delegates() {
        let ctx = SpanContext::new_root();
        let headers = propagate_headers(&ctx);
        assert!(headers.contains_key("x-trace-id"));
    }
}
