//! Minimaler In-Memory-Rate-Limiter (Sliding Window) für die Auth-Endpunkte
//! (PLAN.md: Brute-Force-Schutz auf register/login). Bewusst einfach —
//! prozesslokal, kein externer Store. Reicht für Single-Instance-Deploy.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct RateLimiter {
    inner: Mutex<HashMap<String, Vec<Instant>>>,
    max: usize,
    window: Duration,
}

impl RateLimiter {
    pub fn new(max: usize, window: Duration) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            max,
            window,
        }
    }

    /// `true` = erlaubt, `false` = Limit überschritten.
    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut map = self.inner.lock().expect("ratelimit mutex");
        let hits = map.entry(key.to_string()).or_default();
        hits.retain(|t| now.duration_since(*t) < self.window);
        if hits.len() >= self.max {
            return false;
        }
        hits.push(now);
        true
    }
}
