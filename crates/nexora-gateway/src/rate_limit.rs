//! Rate limiting middleware — تحديد معدل الطلبات لكل IP.
//!
//! يستخدم خوارزمية "sliding window" لتتبع عدد الطلبات في نافذة زمنية.
//! عند تجاوز الحد، يُرجع 429 Too Many Requests.

use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// إعدادات rate limiting.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// الحد الأقصى للطلبات في النافذة.
    pub max_requests: usize,
    /// حجم النافذة الزمنية.
    pub window: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window: Duration::from_secs(60),
        }
    }
}

/// حالة rate limiting لـ IP واحد.
#[derive(Debug, Clone)]
struct IpState {
    /// عدد الطلبات.
    count: usize,
    /// بداية النافذة.
    window_start: Instant,
}

/// مدير rate limiting.
#[derive(Clone)]
pub struct RateLimiter {
    config: RateLimitConfig,
    states: Arc<RwLock<HashMap<String, IpState>>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// تحقق من إمكانية قبول الطلب من هذا IP.
    /// يُرجع `true` إن سُمح، `false` إن تجاوز الحد.
    pub fn check(&self, ip: &str) -> bool {
        let now = Instant::now();
        let mut states = self.states.write();

        let state = states.entry(ip.to_string()).or_insert(IpState {
            count: 0,
            window_start: now,
        });

        // أعد ضبط النافذة إن انتهت.
        if now.duration_since(state.window_start) > self.config.window {
            state.count = 0;
            state.window_start = now;
        }

        state.count += 1;
        state.count <= self.config.max_requests
    }

    /// عدد الطلبات المتبقية لـ IP.
    pub fn remaining(&self, ip: &str) -> usize {
        let states = self.states.read();
        if let Some(state) = states.get(ip) {
            let now = Instant::now();
            if now.duration_since(state.window_start) > self.config.window {
                return self.config.max_requests;
            }
            self.config.max_requests.saturating_sub(state.count)
        } else {
            self.config.max_requests
        }
    }

    /// نظّف الحالات القديمة (لمنع تسرب الذاكرة).
    pub fn cleanup(&self) -> usize {
        let now = Instant::now();
        let mut states = self.states.write();
        let before = states.len();
        states.retain(|_, state| {
            now.duration_since(state.window_start) <= self.config.window * 2
        });
        before - states.len()
    }

    /// عدد الـ IPs النشطة.
    pub fn active_ips(&self) -> usize {
        self.states.read().len()
    }
}

/// استخراج IP من الطلب.
fn extract_ip<B>(req: &Request<B>) -> String {
    // جرّب X-Forwarded-For أولاً (خلف proxy).
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(s) = forwarded.to_str() {
            if let Some(first_ip) = s.split(',').next() {
                return first_ip.trim().to_string();
            }
        }
    }
    // جرّب X-Real-IP.
    if let Some(real) = req.headers().get("x-real-ip") {
        if let Ok(s) = real.to_str() {
            return s.trim().to_string();
        }
    }
    // الافتراضي.
    "unknown".to_string()
}

/// برمجية rate limiting الوسيطة.
pub async fn rate_limit_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // استخرج RateLimiter من امتدادات الطلب.
    let limiter = req
        .extensions()
        .get::<RateLimiter>()
        .cloned();

    let limiter = match limiter {
        Some(l) => l,
        None => {
            // إن لم يُهيّأ، اسمح بالمرور.
            return next.run(req).await;
        }
    };

    let ip = extract_ip(&req);
    if !limiter.check(&ip) {
        let remaining = limiter.remaining(&ip);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [
                ("x-ratelimit-limit", limiter.config.max_requests.to_string()),
                ("x-ratelimit-remaining", remaining.to_string()),
                ("retry-after", "60".to_string()),
            ],
            axum::Json(serde_json::json!({
                "ok": false,
                "error": "تم تجاوز حد الطلبات. حاول مرة أخرى لاحقاً.",
                "retry_after_seconds": 60,
            })),
        )
            .into_response();
    }

    let remaining = limiter.remaining(&ip);
    let mut response = next.run(req).await;
    response.headers_mut().insert(
        "x-ratelimit-limit",
        limiter.config.max_requests.to_string().parse().unwrap(),
    );
    response.headers_mut().insert(
        "x-ratelimit-remaining",
        remaining.to_string().parse().unwrap(),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn allows_under_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 5,
            window: Duration::from_secs(60),
        });
        for _ in 0..5 {
            assert!(limiter.check("1.2.3.4"));
        }
    }

    #[test]
    fn blocks_over_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 3,
            window: Duration::from_secs(60),
        });
        for _ in 0..3 {
            assert!(limiter.check("1.2.3.4"));
        }
        assert!(!limiter.check("1.2.3.4"));
    }

    #[test]
    fn different_ips_independent() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 2,
            window: Duration::from_secs(60),
        });
        assert!(limiter.check("1.1.1.1"));
        assert!(limiter.check("1.1.1.1"));
        assert!(!limiter.check("1.1.1.1"));

        assert!(limiter.check("2.2.2.2"));
    }

    #[test]
    fn window_reset() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 2,
            window: Duration::from_millis(100),
        });
        assert!(limiter.check("1.1.1.1"));
        assert!(limiter.check("1.1.1.1"));
        assert!(!limiter.check("1.1.1.1"));

        sleep(Duration::from_millis(150));
        assert!(limiter.check("1.1.1.1"));
    }

    #[test]
    fn remaining_calculations() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 5,
            window: Duration::from_secs(60),
        });
        assert_eq!(limiter.remaining("1.1.1.1"), 5);
        limiter.check("1.1.1.1");
        assert_eq!(limiter.remaining("1.1.1.1"), 4);
        limiter.check("1.1.1.1");
        assert_eq!(limiter.remaining("1.1.1.1"), 3);
    }

    #[test]
    fn remaining_for_unknown_ip_is_max() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 10,
            window: Duration::from_secs(60),
        });
        assert_eq!(limiter.remaining("unknown"), 10);
    }

    #[test]
    fn active_ips_count() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 10,
            window: Duration::from_secs(60),
        });
        assert_eq!(limiter.active_ips(), 0);
        limiter.check("1.1.1.1");
        limiter.check("2.2.2.2");
        assert_eq!(limiter.active_ips(), 2);
    }

    #[test]
    fn cleanup_removes_old() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 10,
            window: Duration::from_millis(50),
        });
        limiter.check("1.1.1.1");
        limiter.check("2.2.2.2");
        assert_eq!(limiter.active_ips(), 2);

        sleep(Duration::from_millis(150));
        let removed = limiter.cleanup();
        assert_eq!(removed, 2);
        assert_eq!(limiter.active_ips(), 0);
    }

    #[test]
    fn cleanup_keeps_recent() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 10,
            window: Duration::from_secs(60),
        });
        limiter.check("1.1.1.1");
        let removed = limiter.cleanup();
        assert_eq!(removed, 0);
        assert_eq!(limiter.active_ips(), 1);
    }

    #[test]
    fn default_config() {
        let cfg = RateLimitConfig::default();
        assert_eq!(cfg.max_requests, 100);
        assert_eq!(cfg.window, Duration::from_secs(60));
    }
}
