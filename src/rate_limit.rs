use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Maximum number of distinct IPs tracked at once. Bounds memory under a
/// distributed spray of login attempts from many source addresses.
const MAX_ENTRIES: usize = 10_000;

/// A per-IP fixed-window rate limiter, used to throttle login attempts.
///
/// State lives in memory only, for the lifetime of the process. A 60-second
/// window has no persistence value: liftlog is a single-process,
/// single-SQLite-file self-hosted service, and writing a database row per
/// login attempt would turn an attacker's brute-force traffic into
/// write-amplification denial-of-service against the same database that
/// serves real users. A restart clearing every counter (letting anyone who
/// was throttled start over) is an accepted trade-off for that simplicity.
pub struct RateLimiter {
    attempts: Mutex<HashMap<IpAddr, (u32, Instant)>>,
    max_attempts: u32,
    window: Duration,
    max_entries: usize,
}

impl RateLimiter {
    pub fn new(max_attempts: u32, window: Duration) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            max_attempts,
            window,
            max_entries: MAX_ENTRIES,
        }
    }

    /// Locks the attempt map, handling poisoning by recovering the
    /// (possibly inconsistent but still usable) inner guard rather than
    /// panicking. A panic inside one request while holding this lock must
    /// not take down every future login attempt on the service.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<IpAddr, (u32, Instant)>> {
        self.attempts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Attempts to reserve one login attempt for `ip`. Returns `true` if the
    /// attempt is allowed (and counts against the budget), `false` if `ip`
    /// has exceeded `max_attempts` within the current window.
    ///
    /// The lock is taken exactly once for the whole check-and-record
    /// operation. Splitting this into a separate "check" then "record" call
    /// would let concurrent requests all observe the same pre-attack count
    /// and all be admitted, defeating the limit.
    pub fn try_acquire(&self, ip: IpAddr) -> bool {
        let mut map = self.lock();

        if map.len() >= self.max_entries && !map.contains_key(&ip) {
            // At capacity and this is a new IP. First try to make room by
            // dropping entries whose window has already expired.
            map.retain(|_, (_, started)| started.elapsed() < self.window);

            if map.len() >= self.max_entries {
                // Still full after pruning expired entries. Let this
                // untracked source through rather than reject it, and leave
                // every existing counter untouched: we deliberately do NOT
                // clear() the map here, because that would hand anyone who
                // is currently throttled a free reset of their own budget
                // simply by having an attacker spray enough fresh IPs to
                // fill the table.
                return true;
            }
        }

        let entry = map.entry(ip).or_insert((0, Instant::now()));
        if entry.1.elapsed() >= self.window {
            *entry = (1, Instant::now());
            true
        } else if entry.0 >= self.max_attempts {
            false
        } else {
            entry.0 += 1;
            true
        }
    }

    /// Hands back a previously reserved attempt, called after a successful
    /// login so a legitimate user signing in repeatedly (new device, cleared
    /// cookies, a test suite) is never locked out. An attacker's attempts
    /// are all failures, so their budget is never released by this path.
    pub fn release(&self, ip: IpAddr) {
        let mut map = self.lock();
        if let std::collections::hash_map::Entry::Occupied(mut occupied) = map.entry(ip) {
            let count = occupied.get_mut();
            count.0 = count.0.saturating_sub(1);
            if count.0 == 0 {
                occupied.remove();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::thread;

    fn ip(last: u8) -> IpAddr {
        IpAddr::from([127, 0, 0, last])
    }

    #[test]
    fn allows_up_to_max_attempts_then_blocks() {
        let limiter = RateLimiter::new(5, Duration::from_secs(60));
        let addr = ip(1);
        for _ in 0..5 {
            assert!(limiter.try_acquire(addr));
        }
        assert!(!limiter.try_acquire(addr));
    }

    #[test]
    fn separate_ips_have_separate_budgets() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        assert!(limiter.try_acquire(ip(1)));
        assert!(!limiter.try_acquire(ip(1)));
        assert!(limiter.try_acquire(ip(2)));
    }

    #[test]
    fn window_expiry_resets_the_budget() {
        // A zero-length window is already elapsed by the time it's checked,
        // so this needs no sleep to exercise the reset path.
        let limiter = RateLimiter::new(1, Duration::ZERO);
        let addr = ip(1);
        assert!(limiter.try_acquire(addr));
        assert!(limiter.try_acquire(addr));
        assert!(limiter.try_acquire(addr));
    }

    #[test]
    fn release_returns_the_reserved_attempt() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        let addr = ip(1);
        for _ in 0..10 {
            assert!(limiter.try_acquire(addr));
            limiter.release(addr);
        }
        let map = limiter.lock();
        assert!(map.is_empty());
    }

    #[test]
    fn map_is_pruned_at_capacity() {
        let mut limiter = RateLimiter::new(5, Duration::from_secs(60));
        limiter.max_entries = 4;
        for i in 0..50u8 {
            limiter.try_acquire(ip(i));
        }
        let map = limiter.lock();
        assert!(map.len() <= 4, "map grew beyond max_entries: {}", map.len());
    }

    #[test]
    fn capacity_spray_does_not_reset_an_existing_counter() {
        let mut limiter = RateLimiter::new(1, Duration::from_secs(60));
        limiter.max_entries = 4;

        let victim = ip(1);
        assert!(limiter.try_acquire(victim));
        assert!(!limiter.try_acquire(victim));

        for i in 2..52u8 {
            limiter.try_acquire(ip(i));
        }

        assert!(
            !limiter.try_acquire(victim),
            "victim's budget must not be reset by a capacity spray from other IPs"
        );
    }

    #[test]
    fn concurrent_acquires_do_not_exceed_the_limit() {
        let max_attempts = 5;
        let threads = 20;
        let limiter = Arc::new(RateLimiter::new(max_attempts, Duration::from_secs(60)));
        let barrier = Arc::new(Barrier::new(threads));
        let addr = ip(1);

        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let limiter = Arc::clone(&limiter);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    limiter.try_acquire(addr)
                })
            })
            .collect();

        let successes = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|&ok| ok)
            .count();

        assert_eq!(successes, max_attempts as usize);
    }
}
