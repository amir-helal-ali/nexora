//! # التتبع الموزّع Nexora
//!
//! تتبع الطلبات عبر الخدمات على نمط OpenTelemetry.
//!
//! # المفاهيم
//!
//! - **Trace**: سلسلة كاملة من العمليات لطلب واحد
//! - **Span**: وحدة عمل واحدة ضمن تتبع (مثلاً: استدعاء DB)
//! - **SpanContext**: معرّف التتبع + معرّف السلسلة (للتPropagation)
//!
//! # مثال
//!
//! ```text
//! Trace: abc123
//!   ├─ Span: HTTP GET /api/billing (root)
//!   │   ├─ Span: auth.verify_token
//!   │   ├─ Span: db.query_invoices
//!   │   └─ Span: serialize_response
//! ```

pub mod span;
pub mod tracer;
pub mod propagation;
pub mod otlp;

pub use span::{Span, SpanId, SpanStatus, SpanKind};
pub use tracer::{Tracer, TraceCollector};
pub use propagation::{SpanContext, TraceId, propagate_headers};
pub use otlp::{export_otlp, span_to_otlp, OtlpExport, OtlpSpan};
