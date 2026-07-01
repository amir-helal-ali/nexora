//! Rate limiting middleware — per-IP and per-user request throttling.
//!
//! See Nexora Engineering Specification, Part 6 (API GATEWAY RULE — Rate
//! limiting) and Part 9 (RATE LIMITING & ABUSE PREVENTION).
//!
//! Uses a sliding window counter per client identifier (IP address or user ID
//! from the AuthContext). Returns HTTP 429 (Too Many Requests) when the limit
//! is exceeded, with standard rate-limit headers.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Configuration for rate limiting.
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    /// Maximum requests per window.
    pub max_requests: u32,
    /// Window duration in seconds.
    pub window_seconds: u64,
    /// Whether to apply rate limiting (false = passthrough).
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_seconds: 60,
            enabled: true,
        }
    }
}

impl fmt::Display for RateLimitConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}req/{}s", self.max_requests, self.window_seconds)
    }
}

/// A counter for a single client.
#[derive(Clone, Debug)]
struct ClientCounter {
    count: u32,
    window_start: Instant,
}

/// Rate limiter state — holds per-client counters.
pub struct RateLimiter {
    config: RateLimitConfig,
    counters: RwLock<HashMap<String, ClientCounter>>,
}

impl fmt::Debug for RateLimiter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tracked = self.counters.read().len();
        f.debug_struct("RateLimiter")
            .field("config", &self.config)
            .field("tracked_clients", &tracked)
            .finish()
    }
}

impl RateLimiter {
    /// Construct a new rate limiter.
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            counters: RwLock::new(HashMap::new()),
        }
    }

    /// Check if a request from the given client ID is allowed.
    pub fn check(&self, client_id: &str) -> (bool, u32, u64) {
        if !self.config.enabled {
            return (true, self.config.max_requests, 0);
        }

        let now = Instant::now();
        let window_duration = Duration::from_secs(self.config.window_seconds);
        let max = self.config.max_requests;

        let mut counters = self.counters.write();

        // Reap expired windows periodically.
        if counters.len() > 10_000 {
            counters.retain(|_, c| now.duration_since(c.window_start) < window_duration);
        }

        let counter = counters.entry(client_id.to_string()).or_insert(ClientCounter {
            count: 0,
            window_start: now,
        });

        if now.duration_since(counter.window_start) >= window_duration {
            counter.count = 0;
            counter.window_start = now;
        }

        counter.count += 1;
        let remaining = if counter.count > max { 0 } else { max - counter.count };
        let elapsed = now.duration_since(counter.window_start);
        let reset = window_duration.saturating_sub(elapsed).as_secs();

        (counter.count <= max, remaining, reset)
    }

    /// Get current stats for a client.
    pub fn get_stats(&self, client_id: &str) -> Option<(u32, u32, u64)> {
        let now = Instant::now();
        let window_duration = Duration::from_secs(self.config.window_seconds);
        let counters = self.counters.read();
        let counter = counters.get(client_id)?;
        if now.duration_since(counter.window_start) >= window_duration {
            return Some((0, self.config.max_requests, 0));
        }
        let remaining = if counter.count > self.config.max_requests {
            0
        } else {
            self.config.max_requests - counter.count
        };
        let reset = window_duration
            .saturating_sub(now.duration_since(counter.window_start))
            .as_secs();
        Some((counter.count, remaining, reset))
    }

    /// Number of tracked clients.
    pub fn tracked_count(&self) -> usize {
        self.counters.read().len()
    }

    /// Get the config.
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Reset all counters.
    pub fn reset(&self) {
        self.counters.write().clear();
    }
}

/// Shared rate limiter state.
#[derive(Clone)]
pub struct RateLimitState {
    limiter: Arc<RateLimiter>,
}

impl RateLimitState {
    /// Construct new state.
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            limiter: Arc::new(RateLimiter::new(config)),
        }
    }

    /// Get the underlying limiter.
    pub fn limiter(&self) -> &Arc<RateLimiter> {
        &self.limiter
    }
}

impl fmt::Debug for RateLimitState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RateLimitState")
            .field("limiter", &self.limiter)
            .finish()
    }
}

/// Extract client ID from request.
fn extract_client_id(req: &Request) -> String {
    // Try AuthContext first (most accurate).
    if let Some(ctx) = req.extensions().get::<crate::middleware::AuthContext>() {
        return format!("user:{}", ctx.user_id);
    }

    // Try X-Forwarded-For.
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(s) = forwarded.to_str() {
            if let Some(first_ip) = s.split(',').next() {
                return format!("ip:{}", first_ip.trim());
            }
        }
    }

    // Try X-Real-IP.
    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(s) = real_ip.to_str() {
            return format!("ip:{}", s);
        }
    }

    "ip:unknown".to_string()
}

/// Rate limiting middleware function.
pub async fn rate_limit_middleware(
    state: axum::extract::State<RateLimitState>,
    req: Request,
    next: Next,
) -> Response {
    let client_id = extract_client_id(&req);
    let (allowed, remaining, reset) = state.limiter.check(&client_id);

    if !allowed {
        let mut response = (
            StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded. Please slow down.",
        )
            .into_response();

        let headers = response.headers_mut();
        headers.insert("x-ratelimit-limit", state.limiter.config().max_requests.into());
        headers.insert("x-ratelimit-remaining", 0u32.into());
        headers.insert("x-ratelimit-reset", reset.into());
        headers.insert("retry-after", reset.into());

        return response;
    }

    let mut response = next.run(req).await;

    let headers = response.headers_mut();
    headers.insert("x-ratelimit-limit", state.limiter.config().max_requests.into());
    headers.insert("x-ratelimit-remaining", remaining.into());
    headers.insert("x-ratelimit-reset", reset.into());

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_under_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 5,
            window_seconds: 60,
            enabled: true,
        });
        for _ in 0..5 {
            let (allowed, _, _) = limiter.check("client-1");
            assert!(allowed);
        }
    }

    #[test]
    fn blocks_over_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 3,
            window_seconds: 60,
            enabled: true,
        });
        for _ in 0..3 {
            limiter.check("client-1");
        }
        let (allowed, remaining, _) = limiter.check("client-1");
        assert!(!allowed);
        assert_eq!(remaining, 0);
    }

    #[test]
    fn different_clients_independent() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 2,
            window_seconds: 60,
            enabled: true,
        });
        limiter.check("client-1");
        limiter.check("client-1");
        let (allowed1, _, _) = limiter.check("client-1");
        assert!(!allowed1);
        let (allowed2, remaining2, _) = limiter.check("client-2");
        assert!(allowed2);
        assert_eq!(remaining2, 1);
    }

    #[test]
    fn disabled_allows_everything() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 1,
            window_seconds: 60,
            enabled: false,
        });
        for _ in 0..100 {
            let (allowed, _, _) = limiter.check("client-1");
            assert!(allowed);
        }
    }

    #[test]
    fn window_reset_after_expiry() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 2,
            window_seconds: 1,
            enabled: true,
        });
        limiter.check("client-1");
        limiter.check("client-1");
        let (allowed, _, _) = limiter.check("client-1");
        assert!(!allowed);
        std::thread::sleep(Duration::from_millis(1100));
        let (allowed, remaining, _) = limiter.check("client-1");
        assert!(allowed);
        assert_eq!(remaining, 1);
    }

    #[test]
    fn get_stats_works() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 10,
            window_seconds: 60,
            enabled: true,
        });
        limiter.check("client-1");
        limiter.check("client-1");
        limiter.check("client-1");
        let (count, remaining, _) = limiter.get_stats("client-1").unwrap();
        assert_eq!(count, 3);
        assert_eq!(remaining, 7);
    }

    #[test]
    fn get_stats_unknown_client() {
        let limiter = RateLimiter::new(RateLimitConfig::default());
        assert!(limiter.get_stats("unknown").is_none());
    }

    #[test]
    fn tracked_count() {
        let limiter = RateLimiter::new(RateLimitConfig::default());
        assert_eq!(limiter.tracked_count(), 0);
        limiter.check("a");
        limiter.check("b");
        assert_eq!(limiter.tracked_count(), 2);
    }

    #[test]
    fn reset_clears_counters() {
        let limiter = RateLimiter::new(RateLimitConfig::default());
        limiter.check("a");
        limiter.check("b");
        assert_eq!(limiter.tracked_count(), 2);
        limiter.reset();
        assert_eq!(limiter.tracked_count(), 0);
    }
}
