
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;

// Tracks last activity time as "milliseconds since start"
#[derive(Clone)]
pub struct ActivityTimer {
    start: Instant,
    last_ms: Arc<AtomicU64>,
    idle_limit: Duration,
}

impl ActivityTimer {
    pub fn new(idle_limit_mins: u64) -> Self {
        ActivityTimer {
            start: Instant::now(),
            last_ms: Arc::new(AtomicU64::new(0)),
            idle_limit: Duration::from_secs(idle_limit_mins * 60),
        }
    }

    // Call this to update the last activity time to now
    pub fn touch(&self) {
        let ms = self.start.elapsed().as_millis() as u64;
        self.last_ms.store(ms, AtomicOrdering::Relaxed);
    }
    
    // Check if idle time has exceeded the limit
    pub fn is_idle(&self) -> bool {
        let now = self.start.elapsed().as_millis() as u64;
        let last = self.last_ms.load(AtomicOrdering::Relaxed);
        let idle_ms = now.saturating_sub(last) as u128;
        idle_ms >= self.idle_limit.as_millis()
    }
}