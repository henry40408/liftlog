use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Maximum number of distinct keys tracked at once. Bounds memory under a
/// distributed spray of attempts from many source addresses (or, for the
/// user-keyed limiter, many accounts).
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

/// The mutex payload: per-key windows, plus one shared window for keys
/// admitted while `entries` is at capacity. See the capacity branch of
/// `try_acquire` for why the overflow bucket exists.
struct Buckets<K> {
    entries: HashMap<K, Window>,
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

/// A per-key fixed-window rate limiter, used to throttle password guesses.
///
/// Generic over the key so the same logic serves both throttles: login is
/// keyed by `IpAddr` (the request is anonymous, so the source address is the
/// only identity available), while the password-change throttle is keyed by
/// user id — that request is authenticated, so the account it targets is
/// known exactly, and keying on it means an attacker holding a stolen session
/// cannot buy more guesses by rotating source addresses.
///
/// State lives in memory only, for the lifetime of the process. A short
/// window has no persistence value: liftlog is a single-process,
/// single-SQLite-file self-hosted service, and writing a database row per
/// login attempt would turn an attacker's brute-force traffic into
/// write-amplification denial-of-service against the same database that
/// serves real users. A restart clearing every counter (letting anyone who
/// was throttled start over) is an accepted trade-off for that simplicity.
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

    /// Locks the bucket state, handling poisoning by recovering the
    /// (possibly inconsistent but still usable) inner guard rather than
    /// panicking. A panic inside one request while holding this lock must
    /// not take down every future login attempt on the service.
    fn lock(&self) -> std::sync::MutexGuard<'_, Buckets<K>> {
        self.buckets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Attempts to reserve one attempt for `key`. Returns `true` if the
    /// attempt is allowed (and counts against the budget), `false` if `key`
    /// has exceeded `max_attempts` within the current window.
    ///
    /// The lock is taken exactly once for the whole check-and-record
    /// operation. Splitting this into a separate "check" then "record" call
    /// would let concurrent requests all observe the same pre-attack count
    /// and all be admitted, defeating the limit.
    pub fn try_acquire(&self, key: K) -> bool {
        let mut buckets = self.lock();

        if buckets.entries.len() >= self.max_entries && !buckets.entries.contains_key(&key) {
            // At capacity and this is a new key. First try to make room by
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
                //     keys to fill the table.
                // Instead, every untracked key while at capacity shares
                // one finite overflow budget: bounded, but not a bypass.
                return charge(&mut buckets.overflow, self.max_attempts, self.window);
            }
        }

        let entry = buckets.entries.entry(key).or_insert_with(Window::new);
        charge(entry, self.max_attempts, self.window)
    }

    /// Hands back a previously reserved attempt, called after a successful
    /// login so a legitimate user signing in repeatedly (new device, cleared
    /// cookies, a test suite) is never locked out. An attacker's attempts
    /// are all failures, so their budget is never released by this path.
    ///
    /// Only ever touches the per-key entry, never the overflow bucket: a
    /// legitimate user who happened to land in the shared overflow bucket
    /// during an active spray (because the table was full at the time) is
    /// collateral damage of being under attack, not a tracked identity this
    /// call can single out. Refunding the overflow bucket on any successful
    /// login would let anyone holding one valid credential keep it
    /// permanently topped up, defeating the whole point of bounding it.
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

    /// The password-change throttle keys by user id (a `String`), not an
    /// `IpAddr`, so pin that the generic parameter actually works for an
    /// owned, non-`Copy` key — including `release`, which moves the key in.
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

/// Consecutive-failure counter that turns repeated failed logins against one
/// account into an escalating delay.
///
/// This is deliberately **not** an account lockout, which is what the OWASP
/// Authentication Cheat Sheet reaches for first. The cheat sheet also warns
/// that lockout is a denial-of-service primitive — anyone can lock anyone out
/// — and suggests letting the forgotten-password flow rescue a locked
/// account. liftlog has no such flow, no email, and its first user is its only
/// administrator, so a hard lockout here would be an unauthenticated attacker
/// permanently locking the owner out of their own data with no recovery path
/// short of editing the database by hand. An escalating delay collapses an
/// attacker's guess rate just as effectively while leaving every legitimate
/// login eventually possible.
///
/// The counter is keyed by the *submitted* username, and is incremented for
/// unknown usernames exactly as for real ones. That symmetry is load-bearing:
/// a delay applied only to accounts that exist would be a user-enumeration
/// oracle measurable with a stopwatch, undoing the constant-cost work in
/// `UserRepository::verify_password`.
///
/// Complements, rather than replaces, the per-IP [`RateLimiter`] on the same
/// route: that one bounds how fast a single source can try, this one bounds
/// how fast *one account* can be tried no matter how many sources are used.
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
    /// The configuration liftlog actually runs, as a named constructor rather
    /// than four literals at the call site in `main` — where nothing can test
    /// them and a typo in the cap would be invisible.
    ///
    /// Three free failures so ordinary mistyping costs nothing, then 1s, 2s,
    /// 4s … capped at 30s, and forgotten after an hour of quiet. The cap is
    /// what a sustained attack settles at: roughly two guesses a minute
    /// against any one account, no matter how many source addresses are
    /// thrown at it.
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
    /// `free_attempts` failures cost nothing — a person mistyping their own
    /// password should not be punished. Past that the delay doubles from
    /// `base`, capped at `max`. An entry untouched for `window` is forgotten.
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

    /// How long this attempt should be held before it is even evaluated.
    ///
    /// Reads without mutating, so a caller that never reaches
    /// [`Self::record_failure`] (because the request failed earlier for an
    /// unrelated reason) does not leave the account penalised.
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

    /// The schedule itself, split out so it can be tested without touching the
    /// map or the clock.
    fn delay_for_count(&self, count: u32) -> Duration {
        // `count` failures have already happened; this is what the *next*
        // attempt waits. With `free_attempts = 3` that means three failures
        // cost nothing and the fourth attempt is the first to wait — so the
        // comparison is `<`, not `<=`. Getting this boundary wrong by one
        // silently hands an attacker a free guess per account.
        if count < self.free_attempts {
            return Duration::ZERO;
        }
        // Saturating rather than wrapping: a long-running attack pushes
        // `count` arbitrarily high, and `base << 40` would overflow into a
        // nonsense duration (or panic in debug).
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
            // Drop everything already past its window first; a spray of
            // one-shot usernames ages out on its own.
            let window = self.window;
            entries.retain(|_, f| f.last.elapsed() < window);

            if entries.len() >= self.max_entries {
                // Still full of *live* entries. Evict the least recently
                // touched one rather than declining to track this key:
                // declining would mean an attacker who first sprayed enough
                // distinct usernames to fill the table could then hammer their
                // real target with no delay at all, which is precisely the
                // attack this exists to stop. Evicting the stalest entry keeps
                // the newest activity tracked.
                //
                // The residual is that a sustained spray can still churn a
                // specific victim's entry out. Reaching that requires keeping
                // 10_000 entries alive inside the window, every one of them
                // paid for with a failed login that also cost the attacker an
                // Argon2 verification and a slot in the per-IP limiter.
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
        // An entry whose window lapsed starts over rather than resuming from
        // an old count — otherwise a single failure months later would inherit
        // the full penalty of a long-forgotten attack.
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
        // Below the free allowance nothing is charged at all.
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

    /// A long-running attack pushes the count arbitrarily high; the shift used
    /// to double the delay must not overflow into a nonsense duration.
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

    /// An entry whose window has lapsed must not be read as a live penalty —
    /// otherwise one failure and a long silence would leave the account
    /// permanently slowed.
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

    /// The count restarts rather than resuming, so a failure long after an old
    /// attack does not inherit its penalty.
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

    /// At capacity the stalest entry is evicted so the newest activity stays
    /// tracked. Declining to track instead would let an attacker fill the
    /// table with throwaway usernames and then hammer their real target with
    /// no delay at all.
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

    /// `delay_for` must not mutate: a request that reads the delay and then
    /// never records a failure (because it succeeded, or failed for an
    /// unrelated reason) must leave the count where it was.
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

    /// Pins the schedule liftlog ships with. `backoff_tests` above proves the
    /// mechanism; this proves the numbers the deployment actually runs, which
    /// otherwise lived as four bare literals in `main` where nothing could
    /// check them.
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
