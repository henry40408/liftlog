package auth

import (
	"sync"
	"time"
)

// maxTrackedKeys bounds memory under a spray from many keys.
const maxTrackedKeys = 10_000

type window struct {
	count   uint32
	started time.Time
}

func (w window) elapsed(span time.Duration) bool {
	return time.Since(w.started) >= span
}

// charge counts one attempt, resetting an elapsed window.
func (w *window) charge(maxAttempts uint32, span time.Duration) bool {
	if w.elapsed(span) {
		*w = window{count: 1, started: time.Now()}
		return true
	}
	if w.count >= maxAttempts {
		return false
	}
	w.count++
	return true
}

// RateLimiter is a per-key fixed-window limiter for password guesses. Login is
// keyed by client IP; password change by user id, so a stolen session cannot
// buy more guesses by rotating addresses.
//
// It is in memory only: persisting would turn brute-force traffic into SQLite
// write amplification. A restart resetting the counters is accepted.
type RateLimiter[K comparable] struct {
	mu          sync.Mutex
	entries     map[K]*window
	overflow    window
	maxAttempts uint32
	span        time.Duration
	maxEntries  int
}

func NewRateLimiter[K comparable](maxAttempts uint32, span time.Duration) *RateLimiter[K] {
	return &RateLimiter[K]{
		entries:     make(map[K]*window),
		overflow:    window{started: time.Now()},
		maxAttempts: maxAttempts,
		span:        span,
		maxEntries:  maxTrackedKeys,
	}
}

// TryAcquire reserves one attempt and reports false if key is over budget.
// Check and record share one lock, so concurrent requests cannot all pass.
func (l *RateLimiter[K]) TryAcquire(key K) bool {
	l.mu.Lock()
	defer l.mu.Unlock()

	entry, tracked := l.entries[key]
	if !tracked && len(l.entries) >= l.maxEntries {
		l.pruneExpired()
	}
	if !tracked && len(l.entries) >= l.maxEntries {
		// Admitting freely would mean unlimited Argon2 guesses, and clearing
		// the map would reset throttled keys, so untracked keys share one
		// finite budget.
		return l.overflow.charge(l.maxAttempts, l.span)
	}
	if !tracked {
		entry = &window{started: time.Now()}
		l.entries[key] = entry
	}
	return entry.charge(l.maxAttempts, l.span)
}

func (l *RateLimiter[K]) pruneExpired() {
	for key, w := range l.entries {
		if w.elapsed(l.span) {
			delete(l.entries, key)
		}
	}
}

// Release refunds an attempt after a successful login, so repeat sign-ins never
// lock out. It never refunds the overflow window: one valid credential could
// otherwise keep it topped up.
func (l *RateLimiter[K]) Release(key K) {
	l.mu.Lock()
	defer l.mu.Unlock()

	entry, tracked := l.entries[key]
	if !tracked {
		return
	}
	if entry.count > 0 {
		entry.count--
	}
	if entry.count == 0 {
		delete(l.entries, key)
	}
}
