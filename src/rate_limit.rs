use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Bounds memory under a spray from many keys.
const MAX_ENTRIES: usize = 10_000;

struct Window {
    count: u32,
    started: Instant,
}

impl Window {
    fn new() -> Self {
        Self {
            count: 0,
            started: Instant::now(),
        }
    }
}

/// Per-key windows, plus one shared window for new keys while at capacity.
struct Buckets<K> {
    entries: HashMap<K, Window>,
    overflow: Window,
}

/// Charges one attempt, resetting an elapsed window.
fn charge(w: &mut Window, max_attempts: u32, window: Duration) -> bool {
    if w.started.elapsed() >= window {
        *w = Window {
            count: 1,
            started: Instant::now(),
        };
        return true;
    }
    if w.count >= max_attempts {
        return false;
    }
    w.count += 1;
    true
}

/// Per-key fixed-window limiter for password guesses. Login is keyed by
/// `IpAddr`; password change by user id, so a stolen session can't buy more
/// guesses by rotating addresses.
///
/// In-memory only: persisting would turn brute-force traffic into SQLite
/// write amplification. A restart resetting counters is accepted.
pub struct RateLimiter<K = std::net::IpAddr> {
    buckets: Mutex<Buckets<K>>,
    max_attempts: u32,
    window: Duration,
    max_entries: usize,
}

impl<K: Eq + Hash> RateLimiter<K> {
    pub fn new(max_attempts: u32, window: Duration) -> Self {
        Self {
            buckets: Mutex::new(Buckets {
                entries: HashMap::new(),
                overflow: Window::new(),
            }),
            max_attempts,
            window,
            max_entries: MAX_ENTRIES,
        }
    }

    /// Recovers from poisoning so one panicked request can't disable login.
    fn lock(&self) -> std::sync::MutexGuard<'_, Buckets<K>> {
        self.buckets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Reserves one attempt; `false` if `key` is over budget. Check and
    /// record share one lock so concurrent requests can't all pass.
    pub fn try_acquire(&self, key: K) -> bool {
        let mut buckets = self.lock();

        if buckets.entries.len() >= self.max_entries && !buckets.entries.contains_key(&key) {
            // New key at capacity: drop expired entries first.
            buckets
                .entries
                .retain(|_, w| w.started.elapsed() < self.window);

            if buckets.entries.len() >= self.max_entries {
                // Still full. Admitting freely means unlimited Argon2
                // guesses; clearing the map resets throttled keys. So
                // untracked keys share one finite overflow budget.
                return charge(&mut buckets.overflow, self.max_attempts, self.window);
            }
        }

        let entry = buckets.entries.entry(key).or_insert_with(Window::new);
        charge(entry, self.max_attempts, self.window)
    }

    /// Refunds an attempt after a successful login so repeat sign-ins never
    /// lock out. Never refunds the overflow bucket: one valid credential
    /// could otherwise keep it topped up.
    pub fn release(&self, key: K) {
        let mut buckets = self.lock();
        if let std::collections::hash_map::Entry::Occupied(mut occupied) =
            buckets.entries.entry(key)
        {
            let w = occupied.get_mut();
            w.count = w.count.saturating_sub(1);
            if w.count == 0 {
                occupied.remove();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;
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
        // A zero window is already elapsed, so no sleep is needed.
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
        let buckets = limiter.lock();
        assert!(buckets.entries.is_empty());
    }

    #[test]
    fn prune_makes_room_when_windows_have_expired() {
        let mut limiter = RateLimiter::new(5, Duration::ZERO);
        limiter.max_entries = 2;

        assert!(limiter.try_acquire(ip(1)));
        assert!(limiter.try_acquire(ip(2)));

        let fresh = ip(3);
        limiter.try_acquire(fresh);

        let buckets = limiter.lock();
        assert!(
            buckets.entries.contains_key(&fresh),
            "pruning expired entries should have made room for the fresh IP"
        );
    }

    #[test]
    fn live_window_at_capacity_diverts_to_overflow() {
        let mut limiter = RateLimiter::new(5, Duration::from_secs(60));
        limiter.max_entries = 2;

        assert!(limiter.try_acquire(ip(1)));
        assert!(limiter.try_acquire(ip(2)));

        let fresh = ip(3);
        limiter.try_acquire(fresh);

        let buckets = limiter.lock();
        assert!(
            !buckets.entries.contains_key(&fresh),
            "a fresh IP at capacity with no expired entries must not get its own slot"
        );
    }

    #[test]
    fn overflow_bucket_is_finite() {
        let mut limiter = RateLimiter::new(1, Duration::from_secs(60));
        limiter.max_entries = 2;

        assert!(limiter.try_acquire(ip(1)));
        assert!(limiter.try_acquire(ip(2)));

        assert!(
            limiter.try_acquire(ip(3)),
            "first untracked source while at capacity should be admitted"
        );
        assert!(
            !limiter.try_acquire(ip(4)),
            "second untracked source while at capacity should share the same finite budget"
        );
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
    fn string_keyed_limiter_tracks_budgets_per_key() {
        let limiter: RateLimiter<String> = RateLimiter::new(2, Duration::from_secs(60));

        assert!(limiter.try_acquire("user-a".to_string()));
        assert!(limiter.try_acquire("user-a".to_string()));
        assert!(
            !limiter.try_acquire("user-a".to_string()),
            "third attempt for the same user must be refused"
        );
        assert!(
            limiter.try_acquire("user-b".to_string()),
            "a different user must have its own budget"
        );

        limiter.release("user-a".to_string());
        assert!(
            limiter.try_acquire("user-a".to_string()),
            "release should hand the attempt back"
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

/// Escalating delay after repeated failed logins against one account.
///
/// Deliberately not a lockout: with no password-reset flow, a lockout would
/// let anyone lock the sole admin out for good.
///
/// Keyed by the *submitted* username, unknown ones included; delaying only
/// real accounts would be a timing enumeration oracle.
///
/// Complements the per-IP [`RateLimiter`]: that bounds one source, this
/// bounds one account across all sources.
pub struct FailureBackoff<K = String> {
    entries: Mutex<HashMap<K, Failures>>,
    free_attempts: u32,
    base: Duration,
    max: Duration,
    window: Duration,
    max_entries: usize,
}

struct Failures {
    count: u32,
    last: Instant,
}

impl FailureBackoff<String> {
    /// Three free failures, then 1s, 2s, 4s … capped at 30s (≈2 guesses/min
    /// per account); forgotten after an hour of quiet.
    pub fn for_login() -> Self {
        Self::new(
            3,
            Duration::from_secs(1),
            Duration::from_secs(30),
            Duration::from_secs(60 * 60),
        )
    }
}

impl<K: Eq + Hash + Clone> FailureBackoff<K> {
    /// `free_attempts` failures cost nothing; then the delay doubles from
    /// `base`, capped at `max`. Entries idle for `window` are forgotten.
    pub fn new(free_attempts: u32, base: Duration, max: Duration, window: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            free_attempts,
            base,
            max,
            window,
            max_entries: MAX_ENTRIES,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<K, Failures>> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Delay before evaluating the next attempt. Read-only, so a request
    /// that never reaches [`Self::record_failure`] penalises nothing.
    pub fn delay_for(&self, key: &K) -> Duration {
        let entries = self.lock();
        let Some(entry) = entries.get(key) else {
            return Duration::ZERO;
        };
        if entry.last.elapsed() >= self.window {
            return Duration::ZERO;
        }
        self.delay_for_count(entry.count)
    }

    fn delay_for_count(&self, count: u32) -> Duration {
        // `<`, not `<=`: with 3 free, the fourth attempt is the first to wait.
        if count < self.free_attempts {
            return Duration::ZERO;
        }
        // Checked, so huge counts clamp to `max` instead of overflowing.
        let doublings = count - self.free_attempts;
        let scaled = self
            .base
            .checked_mul(1u32.checked_shl(doublings).unwrap_or(u32::MAX))
            .unwrap_or(self.max);
        scaled.min(self.max)
    }

    pub fn record_failure(&self, key: K) {
        let mut entries = self.lock();

        if entries.len() >= self.max_entries && !entries.contains_key(&key) {
            let window = self.window;
            entries.retain(|_, f| f.last.elapsed() < window);

            if entries.len() >= self.max_entries {
                // Evict the stalest rather than skip tracking: skipping
                // would let a username spray exempt the real target.
                if let Some(stalest) = entries
                    .iter()
                    .min_by_key(|(_, f)| f.last)
                    .map(|(k, _)| k.clone())
                {
                    entries.remove(&stalest);
                }
            }
        }

        let entry = entries.entry(key).or_insert(Failures {
            count: 0,
            last: Instant::now(),
        });
        // A lapsed entry restarts rather than inheriting an old penalty.
        if entry.last.elapsed() >= self.window {
            entry.count = 0;
        }
        entry.count = entry.count.saturating_add(1);
        entry.last = Instant::now();
    }

    /// Clears the penalty after a proven-correct password.
    pub fn reset(&self, key: &K) {
        self.lock().remove(key);
    }
}

#[cfg(test)]
mod backoff_tests {
    use super::*;

    fn backoff() -> FailureBackoff<String> {
        FailureBackoff::new(
            3,
            Duration::from_secs(1),
            Duration::from_secs(30),
            Duration::from_secs(3600),
        )
    }

    #[test]
    fn free_attempts_cost_nothing() {
        let b = backoff();
        let key = "alice".to_string();
        for _ in 0..3 {
            assert_eq!(b.delay_for(&key), Duration::ZERO);
            b.record_failure(key.clone());
        }
        assert_eq!(
            b.delay_for(&key),
            Duration::from_secs(1),
            "the first failure past the free allowance should start the delay"
        );
    }

    #[test]
    fn delay_doubles_then_saturates_at_the_cap() {
        let b = backoff();
        for count in 0..3 {
            assert_eq!(b.delay_for_count(count), Duration::ZERO);
        }
        let expected = [1, 2, 4, 8, 16, 30, 30, 30];
        for (i, secs) in expected.iter().enumerate() {
            let count = 3 + u32::try_from(i).unwrap();
            assert_eq!(
                b.delay_for_count(count),
                Duration::from_secs(*secs),
                "after {count} failures the next attempt should wait {secs}s"
            );
        }
    }

    #[test]
    fn absurd_failure_counts_stay_at_the_cap() {
        let b = backoff();
        for count in [40u32, 100, 1000, u32::MAX] {
            assert_eq!(
                b.delay_for_count(count),
                Duration::from_secs(30),
                "count {count} should clamp to the cap"
            );
        }
    }

    #[test]
    fn a_proven_password_clears_the_penalty() {
        let b = backoff();
        let key = "alice".to_string();
        for _ in 0..6 {
            b.record_failure(key.clone());
        }
        assert!(b.delay_for(&key) > Duration::ZERO);

        b.reset(&key);
        assert_eq!(b.delay_for(&key), Duration::ZERO);
    }

    #[test]
    fn accounts_are_penalised_independently() {
        let b = backoff();
        for _ in 0..6 {
            b.record_failure("alice".to_string());
        }
        assert!(b.delay_for(&"alice".to_string()) > Duration::ZERO);
        assert_eq!(b.delay_for(&"bob".to_string()), Duration::ZERO);
    }

    #[test]
    fn a_lapsed_entry_stops_delaying() {
        let b = FailureBackoff::<String>::new(
            0,
            Duration::from_secs(1),
            Duration::from_secs(30),
            Duration::ZERO,
        );
        let key = "alice".to_string();
        b.record_failure(key.clone());
        assert_eq!(
            b.delay_for(&key),
            Duration::ZERO,
            "a zero-length window is already lapsed when it is read"
        );
    }

    #[test]
    fn a_lapsed_entry_restarts_its_count() {
        let b = FailureBackoff::<String>::new(
            0,
            Duration::from_secs(1),
            Duration::from_secs(30),
            Duration::ZERO,
        );
        let key = "alice".to_string();
        for _ in 0..5 {
            b.record_failure(key.clone());
        }
        assert_eq!(
            b.lock().get(&key).unwrap().count,
            1,
            "each record should have found the window lapsed and started over"
        );
    }

    #[test]
    fn at_capacity_the_stalest_entry_is_evicted_for_a_new_one() {
        let mut b = FailureBackoff::<String>::new(
            0,
            Duration::from_secs(1),
            Duration::from_secs(30),
            Duration::from_secs(3600),
        );
        b.max_entries = 2;

        b.record_failure("stale".to_string());
        b.record_failure("fresher".to_string());
        b.record_failure("fresher".to_string());

        let victim = "victim".to_string();
        b.record_failure(victim.clone());

        let entries = b.lock();
        assert!(entries.contains_key(&victim), "the new key must be tracked");
        assert!(
            !entries.contains_key("stale"),
            "the least recently touched key should have been evicted"
        );
        assert!(
            entries.contains_key("fresher"),
            "a more recently touched key should survive"
        );
        assert!(entries.len() <= 2);
    }

    #[test]
    fn reading_the_delay_does_not_penalise() {
        let b = backoff();
        let key = "alice".to_string();
        b.record_failure(key.clone());
        for _ in 0..10 {
            let _ = b.delay_for(&key);
        }
        assert_eq!(b.lock().get(&key).unwrap().count, 1);
    }
}

#[cfg(test)]
mod production_backoff_tests {
    use super::*;

    /// Pins the shipped schedule, not just the mechanism.
    #[test]
    fn for_login_matches_the_documented_schedule() {
        let b = FailureBackoff::for_login();

        for count in 0..3 {
            assert_eq!(
                b.delay_for_count(count),
                Duration::ZERO,
                "three failures should be free"
            );
        }
        assert_eq!(b.delay_for_count(3), Duration::from_secs(1));
        assert_eq!(b.delay_for_count(4), Duration::from_secs(2));
        assert_eq!(b.delay_for_count(5), Duration::from_secs(4));
        assert_eq!(
            b.delay_for_count(u32::MAX),
            Duration::from_secs(30),
            "a sustained attack should settle at the 30s cap"
        );
        assert_eq!(b.window, Duration::from_secs(60 * 60));
    }
}
