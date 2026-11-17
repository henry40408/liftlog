package auth

import (
	"fmt"
	"net/netip"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func ip(last byte) netip.Addr {
	return netip.AddrFrom4([4]byte{127, 0, 0, last})
}

func TestAllowsUpToMaxAttemptsThenBlocks(t *testing.T) {
	limiter := NewRateLimiter[netip.Addr](5, time.Minute)
	for range 5 {
		if !limiter.TryAcquire(ip(1)) {
			t.Fatal("attempt within budget refused")
		}
	}
	if limiter.TryAcquire(ip(1)) {
		t.Fatal("attempt over budget admitted")
	}
}

func TestSeparateKeysHaveSeparateBudgets(t *testing.T) {
	limiter := NewRateLimiter[netip.Addr](1, time.Minute)
	if !limiter.TryAcquire(ip(1)) || limiter.TryAcquire(ip(1)) {
		t.Fatal("first key should get exactly one attempt")
	}
	if !limiter.TryAcquire(ip(2)) {
		t.Fatal("second key should have its own budget")
	}
}

func TestWindowExpiryResetsTheBudget(t *testing.T) {
	// A zero window is already elapsed, so no sleep is needed.
	limiter := NewRateLimiter[netip.Addr](1, 0)
	for range 3 {
		if !limiter.TryAcquire(ip(1)) {
			t.Fatal("an elapsed window should admit again")
		}
	}
}

func TestReleaseReturnsTheReservedAttempt(t *testing.T) {
	limiter := NewRateLimiter[netip.Addr](2, time.Minute)
	for range 10 {
		if !limiter.TryAcquire(ip(1)) {
			t.Fatal("released attempt was not refunded")
		}
		limiter.Release(ip(1))
	}
	if len(limiter.entries) != 0 {
		t.Fatalf("entries = %d, want 0", len(limiter.entries))
	}
}

func smallLimiter(maxAttempts uint32, span time.Duration, maxEntries int) *RateLimiter[netip.Addr] {
	limiter := NewRateLimiter[netip.Addr](maxAttempts, span)
	limiter.maxEntries = maxEntries
	return limiter
}

func TestPruneMakesRoomWhenWindowsHaveExpired(t *testing.T) {
	limiter := smallLimiter(5, 0, 2)
	limiter.TryAcquire(ip(1))
	limiter.TryAcquire(ip(2))
	limiter.TryAcquire(ip(3))
	if _, ok := limiter.entries[ip(3)]; !ok {
		t.Fatal("pruning expired entries should have made room for the fresh IP")
	}
}

func TestLiveWindowAtCapacityDivertsToOverflow(t *testing.T) {
	limiter := smallLimiter(5, time.Minute, 2)
	limiter.TryAcquire(ip(1))
	limiter.TryAcquire(ip(2))
	limiter.TryAcquire(ip(3))
	if _, ok := limiter.entries[ip(3)]; ok {
		t.Fatal("a fresh IP at capacity with no expired entries must not get its own slot")
	}
}

func TestOverflowBucketIsFinite(t *testing.T) {
	limiter := smallLimiter(1, time.Minute, 2)
	limiter.TryAcquire(ip(1))
	limiter.TryAcquire(ip(2))
	if !limiter.TryAcquire(ip(3)) {
		t.Fatal("first untracked source at capacity should be admitted")
	}
	if limiter.TryAcquire(ip(4)) {
		t.Fatal("second untracked source at capacity should share the same finite budget")
	}
}

func TestCapacitySprayDoesNotResetAnExistingCounter(t *testing.T) {
	limiter := smallLimiter(1, time.Minute, 4)
	victim := ip(1)
	if !limiter.TryAcquire(victim) || limiter.TryAcquire(victim) {
		t.Fatal("victim should get exactly one attempt")
	}
	for i := byte(2); i < 52; i++ {
		limiter.TryAcquire(ip(i))
	}
	if limiter.TryAcquire(victim) {
		t.Fatal("victim's budget was reset by a capacity spray from other IPs")
	}
}

func TestStringKeyedLimiterTracksBudgetsPerKey(t *testing.T) {
	limiter := NewRateLimiter[string](2, time.Minute)
	for range 2 {
		if !limiter.TryAcquire("user-a") {
			t.Fatal("two attempts should be admitted")
		}
	}
	if limiter.TryAcquire("user-a") {
		t.Fatal("third attempt for the same user must be refused")
	}
	if !limiter.TryAcquire("user-b") {
		t.Fatal("a different user must have its own budget")
	}
	limiter.Release("user-a")
	if !limiter.TryAcquire("user-a") {
		t.Fatal("release should hand the attempt back")
	}
}

func TestConcurrentAcquiresDoNotExceedTheLimit(t *testing.T) {
	const maxAttempts, goroutines = 5, 20
	limiter := NewRateLimiter[netip.Addr](maxAttempts, time.Minute)
	var successes atomic.Int32
	var ready, done sync.WaitGroup
	ready.Add(1)
	for range goroutines {
		done.Go(func() {
			ready.Wait()
			if limiter.TryAcquire(ip(1)) {
				successes.Add(1)
			}
		})
	}
	ready.Done()
	done.Wait()
	if got := successes.Load(); got != maxAttempts {
		t.Fatalf("successes = %d, want %d", got, maxAttempts)
	}
}

func testBackoff() *FailureBackoff[string] {
	return NewFailureBackoff[string](BackoffSchedule{
		FreeAttempts: 3, Base: time.Second, Max: 30 * time.Second, Window: time.Hour,
	})
}

func TestFreeAttemptsCostNothing(t *testing.T) {
	b := testBackoff()
	for range 3 {
		if got := b.DelayFor("alice"); got != 0 {
			t.Fatalf("delay = %v, want 0", got)
		}
		b.RecordFailure("alice")
	}
	if got := b.DelayFor("alice"); got != time.Second {
		t.Fatalf("the first failure past the free allowance should start the delay, got %v", got)
	}
}

func TestDelayDoublesThenSaturatesAtTheCap(t *testing.T) {
	b := testBackoff()
	for count := range uint32(3) {
		if got := b.delayForCount(count); got != 0 {
			t.Fatalf("count %d: delay = %v, want 0", count, got)
		}
	}
	for i, secs := range []int{1, 2, 4, 8, 16, 30, 30, 30} {
		count := uint32(3 + i)
		if got := b.delayForCount(count); got != time.Duration(secs)*time.Second {
			t.Fatalf("after %d failures delay = %v, want %ds", count, got, secs)
		}
	}
}

func TestAbsurdFailureCountsStayAtTheCap(t *testing.T) {
	b := testBackoff()
	for _, count := range []uint32{40, 65, 100, 1000, ^uint32(0)} {
		if got := b.delayForCount(count); got != 30*time.Second {
			t.Fatalf("count %d: delay = %v, want the cap", count, got)
		}
	}
}

func TestAProvenPasswordClearsThePenalty(t *testing.T) {
	b := testBackoff()
	for range 6 {
		b.RecordFailure("alice")
	}
	if b.DelayFor("alice") <= 0 {
		t.Fatal("six failures should delay")
	}
	b.Reset("alice")
	if got := b.DelayFor("alice"); got != 0 {
		t.Fatalf("delay after reset = %v", got)
	}
}

func TestAccountsArePenalisedIndependently(t *testing.T) {
	b := testBackoff()
	for range 6 {
		b.RecordFailure("alice")
	}
	if b.DelayFor("alice") <= 0 || b.DelayFor("bob") != 0 {
		t.Fatal("only alice should be delayed")
	}
}

func lapsingBackoff() *FailureBackoff[string] {
	return NewFailureBackoff[string](BackoffSchedule{Base: time.Second, Max: 30 * time.Second})
}

func TestALapsedEntryStopsDelaying(t *testing.T) {
	b := lapsingBackoff()
	b.RecordFailure("alice")
	if got := b.DelayFor("alice"); got != 0 {
		t.Fatalf("a zero-length window is already lapsed when read, got %v", got)
	}
}

func TestALapsedEntryRestartsItsCount(t *testing.T) {
	b := lapsingBackoff()
	for range 5 {
		b.RecordFailure("alice")
	}
	if got := b.entries["alice"].count; got != 1 {
		t.Fatalf("count = %d, want 1", got)
	}
}

func TestAtCapacityTheStalestEntryIsEvictedForANewOne(t *testing.T) {
	b := testBackoff()
	b.schedule.FreeAttempts = 0
	b.maxEntries = 2

	b.RecordFailure("stale")
	b.RecordFailure("fresher")
	b.RecordFailure("fresher")
	b.RecordFailure("victim")

	if _, ok := b.entries["victim"]; !ok {
		t.Error("the new key must be tracked")
	}
	if _, ok := b.entries["stale"]; ok {
		t.Error("the least recently touched key should have been evicted")
	}
	if _, ok := b.entries["fresher"]; !ok {
		t.Error("a more recently touched key should survive")
	}
	if len(b.entries) > 2 {
		t.Errorf("entries = %d, want at most 2", len(b.entries))
	}
}

func TestReadingTheDelayDoesNotPenalise(t *testing.T) {
	b := testBackoff()
	b.RecordFailure("alice")
	for range 10 {
		b.DelayFor("alice")
	}
	if got := b.entries["alice"].count; got != 1 {
		t.Fatalf("count = %d, want 1", got)
	}
}

func TestLoginBackoffScheduleMatchesTheDocumentedSchedule(t *testing.T) {
	b := NewFailureBackoff[string](LoginBackoffSchedule())
	want := map[uint32]time.Duration{
		0: 0, 1: 0, 2: 0,
		3: time.Second, 4: 2 * time.Second, 5: 4 * time.Second,
		^uint32(0): 30 * time.Second,
	}
	for count, delay := range want {
		if got := b.delayForCount(count); got != delay {
			t.Errorf("count %d: delay = %v, want %v", count, got, delay)
		}
	}
	if b.schedule.Window != time.Hour {
		t.Errorf("window = %v, want 1h", b.schedule.Window)
	}
}

func TestBackoffDoesNotLeakKeysAcrossCapacity(t *testing.T) {
	b := testBackoff()
	b.maxEntries = 3
	for i := range 20 {
		b.RecordFailure(fmt.Sprintf("user-%d", i))
	}
	if len(b.entries) > 3 {
		t.Fatalf("entries = %d, want at most 3", len(b.entries))
	}
}
