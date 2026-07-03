//! Tracing middleware — يتتبع كل طلب HTTP كـ Span تلقائياً.
//!
//! كل طلب يُنشئ Span جذر (root span) مع:
//! - اسم: HTTP method + path
//! - سمات: method, path, status, duration
//! - يُسجّل في Tracer تلقائياً

use crate::routes::GatewayState;
use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use nexora_tracing::{Span, SpanKind, SpanStatus, Tracer};

/// Tracing middleware — ينشئ Span لكل طلب.
pub async fn tracing_middleware(
    State(state): State<GatewayState>,
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let span_name = format!("{method} {path}");

    // أنشئ Span جذر.
    let mut span = Span::root(span_name, SpanKind::Server)
        .with_attribute("http.method", method)
        .with_attribute("http.path", path.clone());

    // نفّذ الطلب.
    let response = next.run(req).await;

    // أضف سمات الاستجابة.
    let status_code = response.status().as_u16();
    span = span.with_attribute("http.status_code", status_code.to_string());

    // أنهِ Span.
    if status_code >= 400 {
        span.end_error();
    } else {
        span.end_ok();
    }

    // سجّل في Tracer.
    state.tracer.finish(span);

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_tracing::{Span, SpanKind, SpanStatus, Tracer};

    #[test]
    fn span_records_attributes() {
        let mut span = Span::root("GET /api/test", SpanKind::Server)
            .with_attribute("http.method", "GET")
            .with_attribute("http.path", "/api/test");
        span.end_ok();
        assert_eq!(span.attributes.get("http.method").unwrap(), "GET");
        assert_eq!(span.attributes.get("http.path").unwrap(), "/api/test");
        assert_eq!(span.status, SpanStatus::Ok);
    }

    #[test]
    fn span_error_on_4xx() {
        let mut span = Span::root("GET /api/test", SpanKind::Server)
            .with_attribute("http.status_code", "404");
        span.end_error();
        assert_eq!(span.status, SpanStatus::Error);
    }

    #[test]
    fn tracer_records_finished_span() {
        let tracer = Tracer::new();
        let mut span = Span::root("GET /api/test", SpanKind::Server);
        span.end_ok();
        let trace_id = span.context.trace_id.clone();
        tracer.finish(span);
        assert_eq!(tracer.collector().trace_count(), 1);
        assert_eq!(tracer.collector().get_trace(&trace_id).len(), 1);
    }

    #[test]
    fn multiple_requests_create_multiple_traces() {
        let tracer = Tracer::new();
        for i in 0..5 {
            let mut span = Span::root(format!("GET /api/test-{i}"), SpanKind::Server);
            span.end_ok();
            tracer.finish(span);
        }
        assert_eq!(tracer.collector().trace_count(), 5);
    }

    #[test]
    fn span_has_duration() {
        let mut span = Span::root("test", SpanKind::Server);
        std::thread::sleep(std::time::Duration::from_millis(5));
        span.end_ok();
        assert!(span.duration_us() >= 5000);
    }
}
