//! Shared request throttle for the distributor adapters.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Block until `interval` has passed since the last request recorded in
/// `last`, then record now. The lock is held across the sleep so concurrent
/// callers queue and each sees the previous caller's timestamp.
pub(super) fn polite_wait(last: &Mutex<Option<Instant>>, interval: Duration) {
    let mut guard = last.lock().expect("throttle mutex poisoned");
    if let Some(prev) = *guard {
        let elapsed = prev.elapsed();
        if elapsed < interval {
            std::thread::sleep(interval - elapsed);
        }
    }
    *guard = Some(Instant::now());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn concurrent_callers_are_spaced_by_the_interval() {
        let interval = Duration::from_millis(100);
        let last = Arc::new(Mutex::new(Some(Instant::now())));

        // Seeded so every thread must sleep and they all wake together.
        let stamps = Arc::new(Mutex::new(Vec::new()));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let (last, stamps) = (Arc::clone(&last), Arc::clone(&stamps));
                std::thread::spawn(move || {
                    polite_wait(&last, interval);
                    stamps.lock().unwrap().push(Instant::now());
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let mut stamps = stamps.lock().unwrap().clone();
        stamps.sort();
        for pair in stamps.windows(2) {
            // 10 ms slack for scheduler jitter.
            assert!(
                pair[1] - pair[0] >= interval - Duration::from_millis(10),
                "two requests only {:?} apart",
                pair[1] - pair[0]
            );
        }
    }
}
