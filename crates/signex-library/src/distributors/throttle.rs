//! Shared polite-request throttle for the distributor adapters.
//!
//! A caller *reserves* the next free slot while holding the lock, then
//! sleeps outside it. Concurrent callers therefore each receive a distinct
//! slot spaced one `interval` apart; none can slip in early the way a
//! read-timestamp / unlock / sleep / overwrite sequence allows.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Minimum spacing between consecutive requests to one distributor.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(1);

/// Spaces calls at least `interval` apart across all threads.
pub struct Throttle {
    interval: Duration,
    /// Earliest instant at which the next caller may proceed.
    next_slot: Mutex<Option<Instant>>,
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            next_slot: Mutex::new(None),
        }
    }

    /// Reserve the next slot (under the lock) and return when it opens.
    fn reserve(&self) -> Instant {
        // The guarded value is a plain `Option<Instant>`; a poisoned lock
        // cannot hold a torn state, so recover it rather than panic.
        let mut guard = self.next_slot.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let slot = guard.map_or(now, |next| next.max(now));
        *guard = Some(slot + self.interval);
        slot
    }

    /// Block until this caller's reserved slot opens. The first call
    /// returns immediately.
    pub fn wait(&self) {
        let slot = self.reserve();
        let now = Instant::now();
        if slot > now {
            std::thread::sleep(slot - now);
        }
    }
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new(DEFAULT_INTERVAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn first_call_is_immediate_second_waits_one_interval() {
        let t = Throttle::new(Duration::from_millis(200));
        let t0 = Instant::now();
        t.wait();
        assert!(t0.elapsed() < Duration::from_millis(100));
        let t1 = Instant::now();
        t.wait();
        assert!(t1.elapsed() >= Duration::from_millis(190));
    }

    #[test]
    fn concurrent_callers_are_spaced_by_at_least_the_interval() {
        let interval = Duration::from_millis(100);
        let throttle = Arc::new(Throttle::new(interval));
        let barrier = Arc::new(Barrier::new(6));
        let handles: Vec<_> = (0..6)
            .map(|_| {
                let (t, b) = (Arc::clone(&throttle), Arc::clone(&barrier));
                std::thread::spawn(move || {
                    b.wait();
                    t.wait();
                    Instant::now()
                })
            })
            .collect();
        let mut times: Vec<Instant> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        times.sort();
        for pair in times.windows(2) {
            let gap = pair[1] - pair[0];
            // Small tolerance for sleep wake-up vs `Instant::now()` skew.
            assert!(
                gap + Duration::from_millis(10) >= interval,
                "callers proceeded {gap:?} apart, expected >= {interval:?}"
            );
        }
    }

    #[test]
    fn reservations_are_strictly_spaced_regardless_of_scheduling() {
        // Deterministic: no sleeping, just the reserved slots.
        let interval = Duration::from_secs(1);
        let t = Throttle::new(interval);
        let slots: Vec<Instant> = (0..5).map(|_| t.reserve()).collect();
        for pair in slots.windows(2) {
            assert!(pair[1] - pair[0] >= interval);
        }
    }
}
