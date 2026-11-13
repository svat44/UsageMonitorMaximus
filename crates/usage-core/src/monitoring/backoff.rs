//! Retry/backoff maths. Pure functions so behaviour is testable without time.

use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct BackoffPolicy {
    pub base: Duration,
    pub cap: Duration,
    /// Fraction (0..=1) by which the delay may be *reduced* at random ("jittered exponential").
    pub jitter: f64,
}

impl BackoffPolicy {
    pub fn for_interval(interval: Duration) -> Self {
        Self {
            base: interval.max(Duration::from_secs(1)),
            cap: Duration::from_secs(15 * 60).max(interval),
            jitter: 0.25,
        }
    }

    /// Delay after the `failures`-th consecutive failure (1-based). `jitter_unit` ∈ [0,1).
    pub fn delay(&self, failures: u32, jitter_unit: f64) -> Duration {
        let exp = failures.saturating_sub(1).min(20);
        let raw = self.base.as_secs_f64() * 2f64.powi(exp as i32);
        let capped = raw.min(self.cap.as_secs_f64());
        let j = (self.jitter.clamp(0.0, 1.0)) * jitter_unit.clamp(0.0, 1.0);
        Duration::from_secs_f64((capped * (1.0 - j)).max(0.0))
    }

    /// Delay after a rate-limit response. A provider-supplied `Retry-After` is a **floor**:
    /// we never retry earlier than told; jitter is only ever added on top (≤10%).
    pub fn rate_limit_delay(&self, retry_after: Option<Duration>, failures: u32, jitter_unit: f64) -> Duration {
        match retry_after {
            Some(ra) => ra + ra.mul_f64(0.10 * jitter_unit.clamp(0.0, 1.0)),
            None => self.delay(failures, jitter_unit).max(self.base),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> BackoffPolicy {
        BackoffPolicy { base: Duration::from_secs(10), cap: Duration::from_secs(300), jitter: 0.25 }
    }

    #[test]
    fn grows_exponentially_and_caps() {
        let d: Vec<u64> = (1..=8).map(|n| p().delay(n, 0.0).as_secs()).collect();
        assert_eq!(d, vec![10, 20, 40, 80, 160, 300, 300, 300]);
    }

    #[test]
    fn jitter_only_reduces_and_is_bounded() {
        let full = p().delay(3, 0.0);
        let j = p().delay(3, 0.999);
        assert!(j < full && j.as_secs_f64() >= full.as_secs_f64() * 0.75 - 0.01);
    }

    #[test]
    fn retry_after_is_a_floor() {
        for u in [0.0, 0.5, 0.999] {
            let d = p().rate_limit_delay(Some(Duration::from_secs(120)), 1, u);
            assert!(d >= Duration::from_secs(120) && d <= Duration::from_secs(132));
        }
    }

    #[test]
    fn no_retry_after_falls_back_to_backoff_not_faster_than_base() {
        assert!(p().rate_limit_delay(None, 1, 0.999) >= Duration::from_secs(10));
    }

    #[test]
    fn huge_failure_count_does_not_overflow() {
        assert_eq!(p().delay(u32::MAX, 0.0), Duration::from_secs(300));
    }
}
