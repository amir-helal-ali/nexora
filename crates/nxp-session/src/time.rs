//! Time helpers for NXP.
//!
//! See RFC §5.1. Frames carry a microsecond timestamp; the receiver accepts
//! frames within ±60 seconds of its own clock to limit replay windows.

use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum allowed clock skew between peers: 60 seconds in microseconds.
pub const MAX_SKEW_US: u64 = 60 * 1_000_000;

/// Current wall-clock time in microseconds since UNIX epoch.
pub fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// Returns `true` if the given frame timestamp is within tolerance of the
/// local clock.
pub fn skew_ok(frame_ts_us: u64) -> bool {
    let now = now_us();
    if now >= frame_ts_us {
        now - frame_ts_us <= MAX_SKEW_US
    } else {
        frame_ts_us - now <= MAX_SKEW_US
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_positive() {
        assert!(now_us() > 1_700_000_000_000_000);
    }

    #[test]
    fn skew_accepts_recent() {
        let now = now_us();
        assert!(skew_ok(now));
        assert!(skew_ok(now - 10_000)); // 10ms ago
        assert!(skew_ok(now + 10_000)); // 10ms ahead
    }

    #[test]
    fn skew_rejects_old() {
        let now = now_us();
        assert!(!skew_ok(now - 2 * MAX_SKEW_US));
        assert!(!skew_ok(now + 2 * MAX_SKEW_US));
    }
}
