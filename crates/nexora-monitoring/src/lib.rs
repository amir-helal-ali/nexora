//! # مراقبة Nexora
//!
//! مقاييس الأداء + فحوصات الصحة + التنبيهات.
//!
//! # المكونات
//!
//! - [`MetricsCollector`]: يجمع مقاييس الطلبات (latency, throughput, errors)
//! - [`HealthProbe`]: فحوصات صحة الخدمات
//! - [`Monitor`]: يجمع كل شيء ويوفر لوحة معلومات

pub mod metrics;
pub mod health;
pub mod monitor;
pub mod prometheus;
pub mod alerts;
pub mod scheduler;

pub use metrics::{MetricsCollector, RequestMetrics, Timer};
pub use health::{HealthProbe, HealthStatus, ProbeResult};
pub use monitor::{Monitor, MonitorSnapshot};
pub use prometheus::export_prometheus;
pub use alerts::{PerformanceAlerter, PerformanceAlert, PerformanceRule, ThresholdType, AlertLevel};
pub use scheduler::{ReportScheduler, ScheduledReport, SchedulePeriod};
