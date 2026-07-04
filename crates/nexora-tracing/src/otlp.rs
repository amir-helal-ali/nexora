//! تصدير بصيغة OpenTelemetry (OTLP JSON).
//!
//! يحوّل Spans إلى صيغة OTLP JSON المتوافقة مع OpenTelemetry Collector.

use crate::span::{Span, SpanStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// رسالة OTLP JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtlpExport {
    pub resource_spans: Vec<ResourceSpans>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSpans {
    pub resource: Resource,
    pub scope_spans: Vec<ScopeSpans>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub attributes: Vec<KeyValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeSpans {
    pub spans: Vec<OtlpSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtlpSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub start_time_unix_nano: String,
    pub end_time_unix_nano: String,
    pub status: OtlpStatus,
    pub attributes: Vec<KeyValue>,
    pub events: Vec<OtlpEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtlpStatus {
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtlpEvent {
    pub name: String,
    pub time_unix_nano: String,
    pub attributes: Vec<KeyValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub value: OtlpValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtlpValue {
    pub string_value: Option<String>,
}

impl OtlpValue {
    pub fn string(s: impl Into<String>) -> Self {
        Self {
            string_value: Some(s.into()),
        }
    }
}

impl KeyValue {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: OtlpValue::string(value),
        }
    }
}

/// تحويل Span إلى صيغة OTLP.
pub fn span_to_otlp(span: &Span) -> OtlpSpan {
    let kind = match span.kind {
        crate::span::SpanKind::Server => "SPAN_KIND_SERVER",
        crate::span::SpanKind::Client => "SPAN_KIND_CLIENT",
        crate::span::SpanKind::Internal => "SPAN_KIND_INTERNAL",
        crate::span::SpanKind::Producer => "SPAN_KIND_PRODUCER",
        crate::span::SpanKind::Consumer => "SPAN_KIND_CONSUMER",
    };

    let code = match span.status {
        SpanStatus::Active => "STATUS_CODE_UNSET",
        SpanStatus::Ok => "STATUS_CODE_OK",
        SpanStatus::Error => "STATUS_CODE_ERROR",
    };

    let end_time = span.end_time.unwrap_or(span.start_time);

    let attributes: Vec<KeyValue> = span
        .attributes
        .iter()
        .map(|(k, v)| KeyValue::new(k, v))
        .collect();

    let events: Vec<OtlpEvent> = span
        .events
        .iter()
        .map(|ev| OtlpEvent {
            name: ev.name.clone(),
            time_unix_nano: ev.timestamp.to_string(),
            attributes: ev
                .attributes
                .iter()
                .map(|(k, v)| KeyValue::new(k, v))
                .collect(),
        })
        .collect();

    OtlpSpan {
        trace_id: span.context.trace_id.clone(),
        span_id: span.span_id.clone(),
        parent_span_id: span.context.parent_span_id.clone(),
        name: span.name.clone(),
        kind: kind.to_string(),
        start_time_unix_nano: span.start_time.to_string(),
        end_time_unix_nano: end_time.to_string(),
        status: OtlpStatus { code: code.to_string() },
        attributes,
        events,
    }
}

/// تصدير قائمة Spans كرسالة OTLP JSON.
pub fn export_otlp(spans: &[Span]) -> String {
    let otlp_spans: Vec<OtlpSpan> = spans.iter().map(span_to_otlp).collect();

    let export = OtlpExport {
        resource_spans: vec![ResourceSpans {
            resource: Resource {
                attributes: vec![KeyValue::new("service.name", "nexora-gateway")],
            },
            scope_spans: vec![ScopeSpans {
                spans: otlp_spans,
            }],
        }],
    };

    serde_json::to_string_pretty(&export).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::{Span, SpanKind};

    #[test]
    fn span_to_otlp_basic() {
        let mut span = Span::root("GET /api/test", SpanKind::Server)
            .with_attribute("http.method", "GET");
        span.end_ok();

        let otlp = span_to_otlp(&span);
        assert_eq!(otlp.name, "GET /api/test");
        assert_eq!(otlp.kind, "SPAN_KIND_SERVER");
        assert_eq!(otlp.status.code, "STATUS_CODE_OK");
        assert!(otlp.parent_span_id.is_none());
    }

    #[test]
    fn span_to_otlp_error() {
        let mut span = Span::root("test", SpanKind::Internal);
        span.end_error();
        let otlp = span_to_otlp(&span);
        assert_eq!(otlp.status.code, "STATUS_CODE_ERROR");
    }

    #[test]
    fn span_to_otlp_attributes() {
        let span = Span::root("test", SpanKind::Client)
            .with_attribute("http.method", "POST")
            .with_attribute("http.url", "/api/billing");
        let otlp = span_to_otlp(&span);
        assert_eq!(otlp.attributes.len(), 2);
    }

    #[test]
    fn span_to_otlp_events() {
        let mut span = Span::root("test", SpanKind::Server);
        span.add_event("cache.miss");
        let otlp = span_to_otlp(&span);
        assert_eq!(otlp.events.len(), 1);
        assert_eq!(otlp.events[0].name, "cache.miss");
    }

    #[test]
    fn span_to_otlp_child() {
        let parent = Span::root("parent", SpanKind::Server);
        let child = Span::child("child", &parent, SpanKind::Internal);
        let otlp = span_to_otlp(&child);
        assert!(otlp.parent_span_id.is_some());
        assert_eq!(otlp.trace_id, parent.context.trace_id);
    }

    #[test]
    fn export_otlp_json_valid() {
        let mut span = Span::root("test", SpanKind::Server);
        span.end_ok();
        let json = export_otlp(&[span]);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["resource_spans"].is_array());
    }

    #[test]
    fn export_otlp_includes_service_name() {
        let span = Span::root("test", SpanKind::Server);
        let json = export_otlp(&[span]);
        assert!(json.contains("nexora-gateway"));
    }

    #[test]
    fn export_otlp_empty() {
        let json = export_otlp(&[]);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["resource_spans"][0]["scope_spans"][0]["spans"].is_array());
    }

    #[test]
    fn kind_mapping() {
        let kinds = vec![
            (SpanKind::Server, "SPAN_KIND_SERVER"),
            (SpanKind::Client, "SPAN_KIND_CLIENT"),
            (SpanKind::Internal, "SPAN_KIND_INTERNAL"),
            (SpanKind::Producer, "SPAN_KIND_PRODUCER"),
            (SpanKind::Consumer, "SPAN_KIND_CONSUMER"),
        ];
        for (kind, expected) in kinds {
            let span = Span::root("test", kind);
            let otlp = span_to_otlp(&span);
            assert_eq!(otlp.kind, expected);
        }
    }

    #[test]
    fn export_multiple_spans() {
        let root = Span::root("root", SpanKind::Server);
        let child = Span::child("child", &root, SpanKind::Internal);
        let json = export_otlp(&[root, child]);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let spans = &parsed["resource_spans"][0]["scope_spans"][0]["spans"];
        assert_eq!(spans.as_array().unwrap().len(), 2);
    }
}
