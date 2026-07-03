//! Span — وحدة عمل واحدة في التتبع.

use crate::propagation::{generate_trace_id, SpanContext, TraceId};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::OffsetDateTime;

/// معرّف Span (16 بايت hex).
pub type SpanId = String;

/// حالة Span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    /// قيد التنفيذ.
    Active,
    /// مكتمل بنجاح.
    Ok,
    /// خطأ.
    Error,
}

impl Default for SpanStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// نوع Span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    /// استقبال طلب (server-side).
    Server,
    /// إرسال طلب (client-side).
    Client,
    /// عملية داخلية.
    Internal,
    /// إنتاج رسالة.
    Producer,
    /// استهلاك رسالة.
    Consumer,
}

impl Default for SpanKind {
    fn default() -> Self {
        Self::Internal
    }
}

/// Span — وحدة عمل في التتبع.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    /// معرّف فريد للـ Span.
    pub span_id: SpanId,
    /// سياق التتبع (trace_id + parent_span_id).
    pub context: SpanContext,
    /// اسم العملية.
    pub name: String,
    /// نوع Span.
    pub kind: SpanKind,
    /// الحالة.
    pub status: SpanStatus,
    /// وقت البداية (unix nanos).
    pub start_time: i64,
    /// وقت النهاية (unix nanos).
    pub end_time: Option<i64>,
    /// المدة (nanos).
    pub duration_nanos: Option<i64>,
    /// السمات (key-value).
    pub attributes: std::collections::HashMap<String, String>,
    /// الأحداث (logs داخل Span).
    pub events: Vec<SpanEvent>,
    /// Spans فرعية.
    pub child_spans: Vec<Span>,
}

/// حدث داخل Span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: i64,
    pub attributes: std::collections::HashMap<String, String>,
}

impl SpanEvent {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            timestamp: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            attributes: std::collections::HashMap::new(),
        }
    }

    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

impl Span {
    /// إنشاء Span جذر (root span).
    pub fn root(name: impl Into<String>, kind: SpanKind) -> Self {
        let trace_id = generate_trace_id();
        let span_id = generate_span_id();
        Self {
            span_id,
            context: SpanContext {
                trace_id,
                parent_span_id: None,
            },
            name: name.into(),
            kind,
            status: SpanStatus::Active,
            start_time: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            end_time: None,
            duration_nanos: None,
            attributes: std::collections::HashMap::new(),
            events: Vec::new(),
            child_spans: Vec::new(),
        }
    }

    /// إنشاء Span فرعي (child span).
    pub fn child(name: impl Into<String>, parent: &Span, kind: SpanKind) -> Self {
        let span_id = generate_span_id();
        Self {
            span_id,
            context: SpanContext {
                trace_id: parent.context.trace_id.clone(),
                parent_span_id: Some(parent.span_id.clone()),
            },
            name: name.into(),
            kind,
            status: SpanStatus::Active,
            start_time: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            end_time: None,
            duration_nanos: None,
            attributes: std::collections::HashMap::new(),
            events: Vec::new(),
            child_spans: Vec::new(),
        }
    }

    /// إضافة سمة.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// إضافة حدث.
    pub fn add_event(&mut self, name: impl Into<String>) {
        self.events.push(SpanEvent::new(name));
    }

    /// إضافة حدث بسمة.
    pub fn add_event_with_attribute(
        &mut self,
        name: impl Into<String>,
        key: impl Into<String>,
        value: impl Into<String>,
    ) {
        self.events
            .push(SpanEvent::new(name).with_attribute(key, value));
    }

    /// إنهاء Span بنجاح.
    pub fn end_ok(&mut self) {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        self.end_time = Some(now);
        self.duration_nanos = Some(now - self.start_time);
        self.status = SpanStatus::Ok;
    }

    /// إنهاء Span بخطأ.
    pub fn end_error(&mut self) {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        self.end_time = Some(now);
        self.duration_nanos = Some(now - self.start_time);
        self.status = SpanStatus::Error;
    }

    /// هل Span مكتمل؟
    pub fn is_finished(&self) -> bool {
        self.end_time.is_some()
    }

    /// المدة بالميكروثانية.
    pub fn duration_us(&self) -> u64 {
        self.duration_nanos
            .map(|n| (n / 1000) as u64)
            .unwrap_or(0)
    }

    /// المدة بالمللي ثانية.
    pub fn duration_ms(&self) -> f64 {
        self.duration_nanos
            .map(|n| n as f64 / 1_000_000.0)
            .unwrap_or(0.0)
    }

    /// إضافة span فرعي.
    pub fn add_child(&mut self, child: Span) {
        self.child_spans.push(child);
    }
}

/// توليد معرّف Span فريد (16 بايت hex).
fn generate_span_id() -> SpanId {
    uuid::Uuid::new_v4().to_string().replace('-', "")[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_span_has_no_parent() {
        let span = Span::root("test", SpanKind::Server);
        assert!(span.context.parent_span_id.is_none());
        assert_eq!(span.status, SpanStatus::Active);
        assert!(!span.span_id.is_empty());
    }

    #[test]
    fn child_span_has_parent() {
        let parent = Span::root("parent", SpanKind::Server);
        let child = Span::child("child", &parent, SpanKind::Internal);
        assert_eq!(
            child.context.parent_span_id,
            Some(parent.span_id.clone())
        );
        assert_eq!(child.context.trace_id, parent.context.trace_id);
    }

    #[test]
    fn end_ok_sets_status() {
        let mut span = Span::root("test", SpanKind::Internal);
        span.end_ok();
        assert_eq!(span.status, SpanStatus::Ok);
        assert!(span.is_finished());
        assert!(span.duration_nanos.is_some());
    }

    #[test]
    fn end_error_sets_status() {
        let mut span = Span::root("test", SpanKind::Internal);
        span.end_error();
        assert_eq!(span.status, SpanStatus::Error);
        assert!(span.is_finished());
    }

    #[test]
    fn attributes_work() {
        let span = Span::root("test", SpanKind::Server)
            .with_attribute("http.method", "GET")
            .with_attribute("http.url", "/api/test");
        assert_eq!(span.attributes.get("http.method").unwrap(), "GET");
        assert_eq!(span.attributes.len(), 2);
    }

    #[test]
    fn add_event() {
        let mut span = Span::root("test", SpanKind::Server);
        span.add_event("cache.miss");
        span.add_event_with_attribute("db.query", "rows", "42");
        assert_eq!(span.events.len(), 2);
        assert_eq!(span.events[1].attributes.get("rows").unwrap(), "42");
    }

    #[test]
    fn duration_us_and_ms() {
        let mut span = Span::root("test", SpanKind::Internal);
        std::thread::sleep(std::time::Duration::from_millis(10));
        span.end_ok();
        assert!(span.duration_us() >= 10_000); // ≥ 10ms = 10000μs
        assert!(span.duration_ms() >= 10.0);
    }

    #[test]
    fn add_child_span() {
        let mut parent = Span::root("parent", SpanKind::Server);
        let child = Span::child("child", &parent, SpanKind::Internal);
        parent.add_child(child);
        assert_eq!(parent.child_spans.len(), 1);
    }

    #[test]
    fn nested_spans_share_trace_id() {
        let root = Span::root("root", SpanKind::Server);
        let child1 = Span::child("child1", &root, SpanKind::Internal);
        let child2 = Span::child("child2", &child1, SpanKind::Internal);

        assert_eq!(root.context.trace_id, child1.context.trace_id);
        assert_eq!(root.context.trace_id, child2.context.trace_id);
    }

    #[test]
    fn span_kind_default() {
        let span = Span::root("test", SpanKind::Internal);
        assert_eq!(span.kind, SpanKind::Internal);
    }

    #[test]
    fn span_status_default() {
        let span = Span::root("test", SpanKind::Internal);
        assert_eq!(span.status, SpanStatus::Active);
    }

    #[test]
    fn span_event_with_multiple_attributes() {
        let event = SpanEvent::new("test")
            .with_attribute("a", "1")
            .with_attribute("b", "2");
        assert_eq!(event.attributes.len(), 2);
    }

    #[test]
    fn unfinished_span_has_no_duration() {
        let span = Span::root("test", SpanKind::Internal);
        assert!(!span.is_finished());
        assert_eq!(span.duration_us(), 0);
        assert_eq!(span.duration_ms(), 0.0);
    }

    #[test]
    fn serde_roundtrip() {
        let span = Span::root("test", SpanKind::Server)
            .with_attribute("key", "value");
        let json = serde_json::to_string(&span).unwrap();
        let back: Span = serde_json::from_str(&json).unwrap();
        assert_eq!(span.span_id, back.span_id);
        assert_eq!(span.name, back.name);
        assert_eq!(back.attributes.get("key").unwrap(), "value");
    }

    #[test]
    fn unique_span_ids() {
        let s1 = Span::root("a", SpanKind::Internal);
        let s2 = Span::root("b", SpanKind::Internal);
        assert_ne!(s1.span_id, s2.span_id);
    }
}
