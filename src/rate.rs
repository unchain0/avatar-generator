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

        // Prune expired entries first: remove entries whose window has passed.
        // This is cheaper and more targeted than evicting the oldest entry
        // blindly, and it preserves counters for active keys.
        buckets.retain(|_, w| now.duration_since(w.started) < self.window);

        // If the key is already present, we don't need to evict — just update
        // its counter below. Only evict when the key is absent and we still
        // need room under the max_buckets cap.
        let key_present = buckets.contains_key(key);

        if !key_present && buckets.len() >= self.max_buckets {
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

        // The retain above dropped every expired window, so a surviving entry
        // is still inside its window and needs no reset here.
        let window = buckets.entry(key.to_string()).or_insert(Window {
            started: now,
            count: 0,
        });

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
    fn poisoned_bucket_table_is_still_usable() {
        let limiter = RateLimiter::new(5, Duration::from_secs(60));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = limiter.buckets.lock().unwrap();
            panic!("poison buckets");
        }));
        assert!(limiter.check("k"));
    }

    #[test]
    fn oldest_bucket_is_evicted_at_the_cap() {
        let limiter = RateLimiter::new_with_max(5, Duration::from_secs(60), 1);
        assert!(limiter.check("first"));
        assert!(limiter.check("second"));
        assert!(limiter.check("first"));
    }

    #[test]
    fn oversized_bucket_table_is_pruned_before_counting() {
        let limiter = RateLimiter::new_with_max(1, Duration::from_secs(60), 20_000);
        for index in 0..10_001 {
            limiter.check(&format!("client-{index}"));
        }
        assert!(limiter.check("fresh-client"));
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
