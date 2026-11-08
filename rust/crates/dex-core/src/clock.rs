//! Monotonic clocks used by the realtime loop.
//!
//! Production uses [`RealClock`]; hermetically driven tests use [`ManualClock`]
//! so endpoint and guard timing can be exercised without sleeping.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Source of monotonic time for the recognition loop.
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

/// Wall-clock source backed by [`Instant::now`].
#[derive(Debug, Default, Clone, Copy)]
pub struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Manually advanced clock for tests.
#[derive(Debug)]
pub struct ManualClock {
    base: Instant,
    offset_ms: AtomicU64,
}

impl ManualClock {
    pub fn new() -> Self {
        Self {
            base: Instant::now(),
            offset_ms: AtomicU64::new(0),
        }
    }

    /// Advances the clock by `ms`. Monotonic by construction.
    pub fn advance(&self, ms: u64) {
        self.offset_ms.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.base + Duration::from_millis(self.offset_ms.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_is_monotonic() {
        let clock = ManualClock::new();
        let start = clock.now();
        clock.advance(250);
        assert_eq!(
            clock.now().duration_since(start),
            Duration::from_millis(250)
        );
        clock.advance(750);
        assert_eq!(
            clock.now().duration_since(start),
            Duration::from_millis(1000)
        );
    }
}
