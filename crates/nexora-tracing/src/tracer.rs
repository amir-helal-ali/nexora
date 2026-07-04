//! Tracer — يدير Spans ويجمعها.

use crate::propagation::TraceId;
use crate::span::{Span, SpanKind, SpanStatus};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// جامع التتبعات — يخزّن كل Spans مكتملة.
pub struct TraceCollector {
    /// Spans مكتملة، مُفهرسة بـ trace_id.
    traces: RwLock<HashMap<TraceId, Vec<Span>>>,
    /// حد أقصى للـ traces المخزّنة.
    max_traces: usize,
}

impl Default for TraceCollector {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl TraceCollector {
    pub fn new(max_traces: usize) -> Self {
        Self {
            traces: RwLock::new(HashMap::new()),
            max_traces,
        }
    }

    /// تسجيل Span مكتمل.
    pub fn record(&self, span: Span) {
        let mut traces = self.traces.write();
        let trace_id = span.context.trace_id.clone();
        traces
            .entry(trace_id)
            .or_default()
            .push(span);

        // إزالة أقدم trace عند التجاوز.
        if traces.len() > self.max_traces {
            if let Some(oldest_id) = traces
                .iter()
                .min_by_key(|(_, spans)| {
                    spans.iter().map(|s| s.start_time).min().unwrap_or(0)
                })
                .map(|(k, _)| k.clone())
            {
                traces.remove(&oldest_id);
            }
        }
    }

    /// الحصول على كل Spans لتتبع محدد.
    pub fn get_trace(&self, trace_id: &str) -> Vec<Span> {
        self.traces
            .read()
            .get(trace_id)
            .cloned()
            .unwrap_or_default()
    }

    /// عدد التتبعات.
    pub fn trace_count(&self) -> usize {
        self.traces.read().len()
    }

    /// إجمالي Spans.
    pub fn span_count(&self) -> usize {
        self.traces.read().values().map(|v| v.len()).sum()
    }

    /// إفراغ الجامع.
    pub fn clear(&self) {
        self.traces.write().clear();
    }

    /// أحدث التتبعات.
    pub fn recent_traces(&self, limit: usize) -> Vec<(TraceId, Vec<Span>)> {
        let traces = self.traces.read();
        let mut entries: Vec<_> = traces
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        entries.sort_by(|a, b| {
            let a_time = a.1.iter().map(|s| s.start_time).max().unwrap_or(0);
            let b_time = b.1.iter().map(|s| s.start_time).max().unwrap_or(0);
            b_time.cmp(&a_time)
        });
        entries.truncate(limit);
        entries
    }
}

/// Tracer — ينشئ ويدير Spans.
pub struct Tracer {
    collector: Arc<TraceCollector>,
}

impl Default for Tracer {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracer {
    pub fn new() -> Self {
        Self {
            collector: Arc::new(TraceCollector::default()),
        }
    }

    pub fn with_collector(collector: Arc<TraceCollector>) -> Self {
        Self { collector }
    }

    /// الوصول إلى الجامع.
    pub fn collector(&self) -> &Arc<TraceCollector> {
        &self.collector
    }

    /// بدء Span جذر.
    pub fn start_root(&self, name: impl Into<String>, kind: SpanKind) -> Span {
        Span::root(name, kind)
    }

    /// بدء Span فرعي.
    pub fn start_child(&self, name: impl Into<String>, parent: &Span, kind: SpanKind) -> Span {
        Span::child(name, parent, kind)
    }

    /// إنهاء Span وتسجيله.
    pub fn finish(&self, mut span: Span) {
        if !span.is_finished() {
            span.end_ok();
        }
        self.collector.record(span);
    }

    /// إنهاء Span بخطأ وتسجيله.
    pub fn finish_error(&self, mut span: Span) {
        span.end_error();
        self.collector.record(span);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracer_start_root() {
        let tracer = Tracer::new();
        let span = tracer.start_root("test", SpanKind::Server);
        assert_eq!(span.status, SpanStatus::Active);
        assert!(span.context.parent_span_id.is_none());
    }

    #[test]
    fn tracer_finish_records() {
        let tracer = Tracer::new();
        let span = tracer.start_root("test", SpanKind::Server);
        let trace_id = span.context.trace_id.clone();
        tracer.finish(span);
        assert_eq!(tracer.collector().trace_count(), 1);
        assert_eq!(tracer.collector().span_count(), 1);
        let trace = tracer.collector().get_trace(&trace_id);
        assert_eq!(trace.len(), 1);
    }

    #[test]
    fn tracer_finish_error() {
        let tracer = Tracer::new();
        let span = tracer.start_root("test", SpanKind::Server);
        tracer.finish_error(span);
        let traces = tracer.collector().recent_traces(1);
        assert_eq!(traces[0].1[0].status, SpanStatus::Error);
    }

    #[test]
    fn collector_multiple_spans_same_trace() {
        let tracer = Tracer::new();
        let root = tracer.start_root("root", SpanKind::Server);
        let trace_id = root.context.trace_id.clone();
        let child = tracer.start_child("child", &root, SpanKind::Internal);
        tracer.finish(root);
        tracer.finish(child);
        assert_eq!(tracer.collector().trace_count(), 1);
        assert_eq!(tracer.collector().span_count(), 2);
        assert_eq!(tracer.collector().get_trace(&trace_id).len(), 2);
    }

    #[test]
    fn collector_multiple_traces() {
        let tracer = Tracer::new();
        for i in 0..5 {
            let span = tracer.start_root(format!("test-{i}"), SpanKind::Server);
            tracer.finish(span);
        }
        assert_eq!(tracer.collector().trace_count(), 5);
        assert_eq!(tracer.collector().span_count(), 5);
    }

    #[test]
    fn collector_clear() {
        let tracer = Tracer::new();
        let span = tracer.start_root("test", SpanKind::Server);
        tracer.finish(span);
        assert_eq!(tracer.collector().trace_count(), 1);
        tracer.collector().clear();
        assert_eq!(tracer.collector().trace_count(), 0);
    }

    #[test]
    fn collector_recent_traces() {
        let tracer = Tracer::new();
        for i in 0..10 {
            let span = tracer.start_root(format!("test-{i}"), SpanKind::Server);
            tracer.finish(span);
        }
        let recent = tracer.collector().recent_traces(3);
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn collector_max_traces_eviction() {
        let collector = Arc::new(TraceCollector::new(3));
        let tracer = Tracer::with_collector(collector);
        for _ in 0..5 {
            let span = tracer.start_root("test", SpanKind::Server);
            tracer.finish(span);
        }
        assert!(tracer.collector().trace_count() <= 3);
    }

    #[test]
    fn get_nonexistent_trace() {
        let tracer = Tracer::new();
        let trace = tracer.collector().get_trace("nonexistent");
        assert!(trace.is_empty());
    }

    #[test]
    fn child_shares_trace_id() {
        let tracer = Tracer::new();
        let root = tracer.start_root("root", SpanKind::Server);
        let child = tracer.start_child("child", &root, SpanKind::Internal);
        assert_eq!(root.context.trace_id, child.context.trace_id);
    }
}
