//! # اختبارات تحميل Nexora
//!
//! أداة لاختبار تحمل البوابة عبر إرسال طلبات متزامنة.
//!
//! # الاستخدام
//!
//! ```no_run
//! use nexora_loadtest::{LoadTest, LoadTestConfig, LoadTestResult};
//!
//! # async fn run() {
//! let config = LoadTestConfig::new("http://localhost:8080/api/health")
//!     .with_concurrent(50)
//!     .with_total(1000);
//! let result = LoadTest::run(config).await;
//! println!("RPS: {:.0}", result.requests_per_second());
//! # }
//! ```

pub mod config;
pub mod result;
pub mod runner;

pub use config::LoadTestConfig;
pub use result::LoadTestResult;
pub use runner::LoadTest;
