package auth

import (
	"sync"
	"time"
)

type failures struct {
	count uint32
	last  time.Time
}

// BackoffSchedule says how the delay grows: FreeAttempts failures cost
// nothing, then the delay doubles from Base up to Max. Entries idle for Window
// are forgotten.
type BackoffSchedule struct {
	FreeAttempts uint32
	Base         time.Duration
	Max          time.Duration
	Window       time.Duration
}

// LoginBackoffSchedule returns three free failures, then 1s, 2s, 4s ... capped at
// 30s (about 2 guesses a minute per account); forgotten after an hour of quiet.
func LoginBackoffSchedule() BackoffSchedule {
	return BackoffSchedule{
		FreeAttempts: 3,
		Base:         time.Second,
		Max:          30 * time.Second,
		Window:       time.Hour,
	}
}

// FailureBackoff is an escalating delay after repeated failed logins against
// one account.
//
// It is deliberately not a lockout: with no password-reset flow, a lockout
// would let anyone lock the sole admin out for good.
//
// Keyed by the submitted username, unknown ones included; delaying only real
// accounts would be a timing enumeration oracle.
//
// It complements the per-IP RateLimiter: that bounds one source, this bounds
// one account across all sources.
type FailureBackoff[K comparable] struct {
	mu         sync.Mutex
	entries    map[K]*failures
	schedule   BackoffSchedule
	maxEntries int
}

func NewFailureBackoff[K comparable](schedule BackoffSchedule) *FailureBackoff[K] {
	return &FailureBackoff[K]{
		entries:    make(map[K]*failures),
		schedule:   schedule,
		maxEntries: maxTrackedKeys,
	}
}

// DelayFor is the delay before evaluating the next attempt. It only reads, so
// a request that never reaches RecordFailure penalises nothing.
func (b *FailureBackoff[K]) DelayFor(key K) time.Duration {
	b.mu.Lock()
	defer b.mu.Unlock()

	entry, ok := b.entries[key]
	if !ok || time.Since(entry.last) >= b.schedule.Window {
		return 0
	}
	return b.delayForCount(entry.count)
}

func (b *FailureBackoff[K]) delayForCount(count uint32) time.Duration {
	// "<", not "<=": with 3 free, the fourth attempt is the first to wait.
	if count < b.schedule.FreeAttempts {
		return 0
	}
	doublings := count - b.schedule.FreeAttempts
	// Checked before shifting, so huge counts clamp to the cap instead of
	// overflowing.
	if doublings >= 62 || b.schedule.Base > b.schedule.Max>>doublings {
		return b.schedule.Max
	}
	return b.schedule.Base << doublings
}

func (b *FailureBackoff[K]) RecordFailure(key K) {
	b.mu.Lock()
	defer b.mu.Unlock()

	entry, tracked := b.entries[key]
	if !tracked && len(b.entries) >= b.maxEntries {
		b.makeRoom()
	}
	if !tracked {
		entry = &failures{last: time.Now()}
		b.entries[key] = entry
	}
	// A lapsed entry restarts rather than inheriting an old penalty.
	if time.Since(entry.last) >= b.schedule.Window {
		entry.count = 0
	}
	if entry.count < ^uint32(0) {
		entry.count++
	}
	entry.last = time.Now()
}

func (b *FailureBackoff[K]) makeRoom() {
	for key, f := range b.entries {
		if time.Since(f.last) >= b.schedule.Window {
			delete(b.entries, key)
		}
	}
	if len(b.entries) < b.maxEntries {
		return
	}
	// Evict the stalest rather than skip tracking: skipping would let a
	// username spray exempt the real target.
	var stalest K
	var stalestTime time.Time
	first := true
	for key, f := range b.entries {
		if first || f.last.Before(stalestTime) {
			stalest, stalestTime, first = key, f.last, false
		}
	}
	delete(b.entries, stalest)
}

// Reset clears the penalty after a proven-correct password.
func (b *FailureBackoff[K]) Reset(key K) {
	b.mu.Lock()
	defer b.mu.Unlock()
	delete(b.entries, key)
}
