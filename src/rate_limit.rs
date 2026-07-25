use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Maximum number of distinct IPs tracked at once. Bounds memory under a
/// distributed spray of login attempts from many source addresses.
const MAX_ENTRIES: usize = 10_000;

/// A single fixed-window attempt counter, shared by both the per-IP entries
/// and the overflow bucket.
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

/// The mutex payload: per-IP windows, plus one shared window for sources
/// admitted while `entries` is at capacity. See the capacity branch of
/// `try_acquire` for why the overflow bucket exists.
struct Buckets {
    entries: HashMap<IpAddr, Window>,
    overflow: Window,
}

/// Applies one attempt against `w`, resetting it if its window has already
/// elapsed. Shared by the per-IP and overflow paths so they cannot drift
/// out of sync with each other.
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
    buckets: Mutex<Buckets>,
    max_attempts: u32,
    window: Duration,
    max_entries: usize,
}

impl RateLimiter {
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

    /// Locks the bucket state, handling poisoning by recovering the
    /// (possibly inconsistent but still usable) inner guard rather than
    /// panicking. A panic inside one request while holding this lock must
    /// not take down every future login attempt on the service.
    fn lock(&self) -> std::sync::MutexGuard<'_, Buckets> {
        self.buckets
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
        let mut buckets = self.lock();

        if buckets.entries.len() >= self.max_entries && !buckets.entries.contains_key(&ip) {
            // At capacity and this is a new IP. First try to make room by
            // dropping entries whose window has already expired.
            buckets
                .entries
                .retain(|_, w| w.started.elapsed() < self.window);

            if buckets.entries.len() >= self.max_entries {
                // Still full after pruning expired entries. This source is
                // untracked and cannot get its own entry. Two tempting
                // alternatives are both wrong:
                //   - Admitting it unconditionally would give it *unlimited*
                //     attempts, each costing a full Argon2 verification —
                //     turning the throttle off for exactly the attacker who
                //     filled the table, plus CPU exhaustion on top.
                //   - Clearing the map to make room would hand every
                //     already-throttled source a free reset of its own
                //     budget, just because an attacker sprayed enough fresh
                //     IPs to fill the table.
                // Instead, every untracked source while at capacity shares
                // one finite overflow budget: bounded, but not a bypass.
                return charge(&mut buckets.overflow, self.max_attempts, self.window);
            }
        }

        let entry = buckets.entries.entry(ip).or_insert_with(Window::new);
        charge(entry, self.max_attempts, self.window)
    }

    /// Hands back a previously reserved attempt, called after a successful
    /// login so a legitimate user signing in repeatedly (new device, cleared
    /// cookies, a test suite) is never locked out. An attacker's attempts
    /// are all failures, so their budget is never released by this path.
    ///
    /// Only ever touches the per-IP entry, never the overflow bucket: a
    /// legitimate user who happened to land in the shared overflow bucket
    /// during an active spray (because the table was full at the time) is
    /// collateral damage of being under attack, not a tracked identity this
    /// call can single out. Refunding the overflow bucket on any successful
    /// login would let anyone holding one valid credential keep it
    /// permanently topped up, defeating the whole point of bounding it.
    pub fn release(&self, ip: IpAddr) {
        let mut buckets = self.lock();
        if let std::collections::hash_map::Entry::Occupied(mut occupied) = buckets.entries.entry(ip)
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
        let buckets = limiter.lock();
        assert!(buckets.entries.is_empty());
    }

    /// Replaces the old (vacuous) `map_is_pruned_at_capacity`: that test
    /// only asserted `map.len() <= max_entries`, which the early `return` in
    /// the capacity branch guarantees regardless of whether pruning actually
    /// runs — deleting the `retain` call entirely still passed it. This
    /// asserts the actual effect of pruning: an expired entry is evicted and
    /// the freed slot is used by a fresh IP.
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

    /// Companion to `prune_makes_room_when_windows_have_expired`: when the
    /// existing entries' windows are still live, pruning frees nothing, so a
    /// fresh IP at capacity must be diverted to the overflow bucket instead
    /// of getting its own `entries` slot.
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

    /// Regression test for the confirmed bypass: previously, once `entries`
    /// was at capacity, every untracked IP was admitted unconditionally —
    /// unlimited attempts for whichever attacker filled the table. Now
    /// untracked sources share one finite overflow bucket: the first is
    /// admitted, the second is refused.
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
