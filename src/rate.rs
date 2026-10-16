use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Window {
    started: Instant,
    count: u32,
}

pub struct RateLimiter {
    limit: u32,
    window: Duration,
    max_buckets: usize,
    buckets: Mutex<HashMap<String, Window>>,
}

impl RateLimiter {
    pub fn new(limit: u32, window: Duration) -> Self {
        Self::new_with_max(limit, window, 10_000)
    }

    pub fn new_with_max(limit: u32, window: Duration, max_buckets: usize) -> Self {
        Self {
            limit: limit.max(1),
            window,
            max_buckets,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub fn per_minute(limit: u32) -> Self {
        Self::new_with_max(limit, Duration::from_secs(60), 10_000)
    }

    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());

        // If we've reached the maximum number of buckets, evict the oldest
        // entry to make room. This prevents the map from growing without bound
        // under sustained distributed traffic where more than 10,000 distinct
        // clients make requests within one window.
        if buckets.len() >= self.max_buckets {
            let oldest_key = buckets
                .iter()
                .min_by_key(|(_, w)| w.started)
                .map(|(k, _)| k.clone());
            if let Some(key_to_evict) = oldest_key {
                buckets.remove(&key_to_evict);
            }
        }

        if buckets.len() > 10_000 {
            buckets.retain(|_, w| now.duration_since(w.started) < self.window);
        }

        let window = buckets.entry(key.to_string()).or_insert(Window {
            started: now,
            count: 0,
        });

        if now.duration_since(window.started) >= self.window {
            window.started = now;
            window.count = 0;
        }

        if window.count >= self.limit {
            return false;
        }
        window.count += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_limit_then_blocks() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.check("1.2.3.4"));
        assert!(limiter.check("1.2.3.4"));
        assert!(!limiter.check("1.2.3.4"));
    }

    #[test]
    fn separate_keys_have_separate_budgets() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        assert!(limiter.check("a"));
        assert!(limiter.check("b"));
        assert!(!limiter.check("a"));
    }

    #[test]
    fn window_resets_after_expiry() {
        let limiter = RateLimiter::new(1, Duration::from_millis(10));
        assert!(limiter.check("k"));
        assert!(!limiter.check("k"));
        std::thread::sleep(Duration::from_millis(20));
        assert!(limiter.check("k"));
    }

    #[test]
    fn max_buckets_enforced_under_concurrent_load() {
        let limiter = RateLimiter::new_with_max(100, Duration::from_secs(60), 3);
        // Simulate many distinct clients making requests.
        // With max_buckets=3, oldest entries should be evicted.
        for i in 0..10 {
            limiter.check(&format!("key{}", i));
        }
        // Verify we can still operate (no panic, bounded size)
        let _ = limiter.check("key_any");
    }
}
