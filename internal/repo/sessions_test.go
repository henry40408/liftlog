package repo

import (
	"context"
	"database/sql"
	"math"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/session"
)

type sessionFixture struct {
	pool   *sql.DB
	repo   *Sessions
	userID string
	ctx    context.Context
}

func newSessionFixture(t *testing.T) sessionFixture {
	t.Helper()
	pool := setupTestDB(t)
	return sessionFixture{
		pool:   pool,
		repo:   NewSessions(pool),
		userID: createRepoUser(t, pool, "testuser").ID,
		ctx:    context.Background(),
	}
}

func (f sessionFixture) create(t *testing.T) string {
	t.Helper()
	token, err := f.repo.Create(f.ctx, f.userID)
	if err != nil {
		t.Fatal(err)
	}
	return token
}

func (f sessionFixture) validate(t *testing.T, token string) ValidateResult {
	t.Helper()
	result, err := f.repo.ValidateAndTouch(f.ctx, token)
	if err != nil {
		t.Fatal(err)
	}
	return result
}

// set bends session columns with SQLite date expressions, like the Rust tests.
func (f sessionFixture) set(t *testing.T, token, assignments string) {
	t.Helper()
	mustExec(t, f.pool, "UPDATE sessions SET "+assignments+" WHERE token = ?", token)
}

func (f sessionFixture) storedTime(t *testing.T, token, column string) time.Time {
	t.Helper()
	var raw string
	if err := f.pool.QueryRow("SELECT "+column+" FROM sessions WHERE token = ?", token).Scan(&raw); err != nil {
		t.Fatal(err)
	}
	parsed, err := db.ParseTime(raw)
	if err != nil {
		t.Fatal(err)
	}
	return parsed
}

func (f sessionFixture) rowCount(t *testing.T, token string) int {
	t.Helper()
	return countRows(t, f.pool, "SELECT COUNT(*) FROM sessions WHERE token = ?", token)
}

func within(a, b time.Time, d time.Duration) bool {
	return math.Abs(float64(a.Sub(b))) < float64(d)
}

func TestSessionsCreateAndValidateWithinThrottleWindow(t *testing.T) {
	f := newSessionFixture(t)
	token := f.create(t)
	if token == "" {
		t.Fatal("empty token")
	}

	result := f.validate(t, token)
	if result.Kind != ValidateValid || result.UserID != f.userID || result.Username != "testuser" {
		t.Fatalf("result = %+v", result)
	}
	if result.NewExpiresAt != nil {
		t.Error("touch should be absorbed by the throttle window")
	}
}

func TestSessionsValidateNonexistent(t *testing.T) {
	f := newSessionFixture(t)
	if got := f.validate(t, "nonexistent-token"); got.Kind != ValidateUnknown {
		t.Errorf("kind = %v, want unknown", got.Kind)
	}
}

func TestSessionsValidateExpiredDeletesRow(t *testing.T) {
	f := newSessionFixture(t)
	token := f.create(t)
	f.set(t, token, "expires_at = datetime('now', '-1 hour')")

	if got := f.validate(t, token); got.Kind != ValidateExpiredIdle {
		t.Fatalf("kind = %v, want expired idle", got.Kind)
	}
	if n := f.rowCount(t, token); n != 0 {
		t.Errorf("row count = %d, want 0", n)
	}
}

func TestSessionsValidateOutsideWindowSlidesExpiry(t *testing.T) {
	f := newSessionFixture(t)
	token := f.create(t)
	f.set(t, token, "last_touched_at = datetime('now', '-2 hours'), expires_at = datetime('now', '+1 day')")
	before := f.storedTime(t, token, "expires_at")

	result := f.validate(t, token)
	if result.Kind != ValidateValid || result.UserID != f.userID {
		t.Fatalf("result = %+v", result)
	}
	if result.NewExpiresAt == nil || !result.NewExpiresAt.After(before) {
		t.Fatalf("NewExpiresAt = %v, want after %v", result.NewExpiresAt, before)
	}
	if got := f.storedTime(t, token, "expires_at"); !within(got, *result.NewExpiresAt, time.Millisecond) {
		t.Errorf("stored expires_at = %v, want %v", got, *result.NewExpiresAt)
	}
	if touched := f.storedTime(t, token, "last_touched_at"); !within(touched, time.Now(), 5*time.Second) {
		t.Errorf("last_touched_at = %v, want ~now", touched)
	}
}

func TestSessionsValidateDeletesSessionPastAbsoluteCap(t *testing.T) {
	f := newSessionFixture(t)
	token := f.create(t)
	// Over the absolute cap with fresh activity: age alone terminates.
	f.set(t, token, "created_at = datetime('now', '-91 days'), last_touched_at = datetime('now'), expires_at = datetime('now', '+1 day')")

	if got := f.validate(t, token); got.Kind != ValidateExpiredAbsolute {
		t.Fatalf("kind = %v, want expired absolute", got.Kind)
	}
	if n := f.rowCount(t, token); n != 0 {
		t.Errorf("row count = %d, want 0", n)
	}
}

func TestSessionsValidateClampsExpiryToAbsoluteCap(t *testing.T) {
	f := newSessionFixture(t)
	token := f.create(t)
	f.set(t, token, "created_at = datetime('now', '-89 days'), last_touched_at = datetime('now', '-2 hours'), expires_at = datetime('now', '+3 hours')")
	createdAt := f.storedTime(t, token, "created_at")

	result := f.validate(t, token)
	if result.Kind != ValidateValid || result.NewExpiresAt == nil {
		t.Fatalf("result = %+v, want a slide below the cap", result)
	}
	if want := session.AbsoluteCap(createdAt); !within(*result.NewExpiresAt, want, 5*time.Second) {
		t.Errorf("new expiry = %v, want ~%v", *result.NewExpiresAt, want)
	}
}

// Pinned to the cap with a stale touch: regression guard for last_touched_at
// freezing once a session hits the cap.
func TestSessionsValidateAtCapStillRecordsActivity(t *testing.T) {
	f := newSessionFixture(t)
	token := f.create(t)
	f.set(t, token, "created_at = datetime('now', '-1 day'), last_touched_at = datetime('now', '-2 hours'), expires_at = datetime('now', '+89 days')")
	expiresBefore := f.storedTime(t, token, "expires_at")

	result := f.validate(t, token)
	if result.Kind != ValidateValid {
		t.Fatalf("kind = %v, want valid", result.Kind)
	}
	if result.NewExpiresAt != nil {
		t.Error("expiry should not slide past the absolute cap")
	}
	if touched := f.storedTime(t, token, "last_touched_at"); !within(touched, time.Now(), 5*time.Second) {
		t.Errorf("last_touched_at = %v, want ~now", touched)
	}
	if after := f.storedTime(t, token, "expires_at"); !after.Equal(expiresBefore) {
		t.Errorf("expires_at changed from %v to %v", expiresBefore, after)
	}
}

func TestSessionsDelete(t *testing.T) {
	f := newSessionFixture(t)
	token := f.create(t)
	if err := f.repo.Delete(f.ctx, token); err != nil {
		t.Fatal(err)
	}
	if got := f.validate(t, token); got.Kind != ValidateUnknown {
		t.Errorf("kind = %v, want unknown", got.Kind)
	}
}

func TestSessionsDeleteAllForUserExcept(t *testing.T) {
	f := newSessionFixture(t)
	token1, token2, token3 := f.create(t), f.create(t), f.create(t)

	count, err := f.repo.DeleteAllForUserExcept(f.ctx, f.userID, token2)
	if err != nil || count != 2 {
		t.Fatalf("count = %d, err = %v; want 2", count, err)
	}
	kinds := []ValidateKind{f.validate(t, token1).Kind, f.validate(t, token2).Kind, f.validate(t, token3).Kind}
	want := []ValidateKind{ValidateUnknown, ValidateValid, ValidateUnknown}
	for i := range kinds {
		if kinds[i] != want[i] {
			t.Errorf("token%d kind = %v, want %v", i+1, kinds[i], want[i])
		}
	}
}

func TestSessionsDeleteAllForUserReturnsCountAndRemovesEveryRow(t *testing.T) {
	f := newSessionFixture(t)
	tokens := []string{f.create(t), f.create(t), f.create(t)}

	count, err := f.repo.DeleteAllForUser(f.ctx, f.userID)
	if err != nil || count != 3 {
		t.Fatalf("count = %d, err = %v; want 3", count, err)
	}
	for _, token := range tokens {
		if got := f.validate(t, token); got.Kind != ValidateUnknown {
			t.Errorf("token still valid: %v", got.Kind)
		}
	}
}

func TestSessionsCountForUserCountsOnlyThatUsersSessions(t *testing.T) {
	f := newSessionFixture(t)
	other := createRepoUser(t, f.pool, "otheruser").ID
	f.create(t)
	f.create(t)
	if _, err := f.repo.Create(f.ctx, other); err != nil {
		t.Fatal(err)
	}

	for userID, want := range map[string]int{f.userID: 2, other: 1, "no-such-user": 0} {
		got, err := f.repo.CountForUser(f.ctx, userID)
		if err != nil || got != want {
			t.Errorf("CountForUser(%s) = %d, %v; want %d", userID, got, err, want)
		}
	}
}

func TestSessionsValidateDistinguishesIdleFromAbsoluteExpiry(t *testing.T) {
	f := newSessionFixture(t)
	idle, absolute := f.create(t), f.create(t)
	f.set(t, idle, "expires_at = datetime('now', '-1 hour')")
	f.set(t, absolute, "created_at = datetime('now', '-91 days'), expires_at = datetime('now', '+1 day')")

	if got := f.validate(t, idle).Kind; got != ValidateExpiredIdle {
		t.Errorf("idle kind = %v", got)
	}
	if got := f.validate(t, absolute).Kind; got != ValidateExpiredAbsolute {
		t.Errorf("absolute kind = %v", got)
	}
	for _, token := range []string{idle, absolute} {
		if n := f.rowCount(t, token); n != 0 {
			t.Errorf("row for %s not deleted", token)
		}
	}
}

func TestSessionsValidateUnknownWhenUserRowIsGone(t *testing.T) {
	f := newSessionFixture(t)
	token := f.create(t)
	// The cascade would delete the session too; either way the token is dead.
	mustExec(t, f.pool, "DELETE FROM users WHERE id = ?", f.userID)

	if got := f.validate(t, token).Kind; got != ValidateUnknown {
		t.Errorf("kind = %v, want unknown", got)
	}
}

func tokensOf(rows []SessionListRow) []string {
	tokens := make([]string, 0, len(rows))
	for _, row := range rows {
		tokens = append(tokens, row.Token)
	}
	return tokens
}

func assertTokens(t *testing.T, rows []SessionListRow, want ...string) {
	t.Helper()
	got := tokensOf(rows)
	if len(got) != len(want) {
		t.Fatalf("tokens = %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("tokens = %v, want %v", got, want)
		}
	}
}

func TestSessionsListForUserNewestTouchedFirst(t *testing.T) {
	f := newSessionFixture(t)
	old, mid, fresh := f.create(t), f.create(t), f.create(t)
	f.set(t, old, "last_touched_at = datetime('now', '-3 days')")
	f.set(t, mid, "last_touched_at = datetime('now', '-1 day')")

	rows, err := f.repo.ListForUser(f.ctx, f.userID)
	if err != nil {
		t.Fatal(err)
	}
	assertTokens(t, rows, fresh, mid, old)
}

func TestSessionsListForUserFiltersExpired(t *testing.T) {
	f := newSessionFixture(t)
	live, dead := f.create(t), f.create(t)
	f.set(t, dead, "expires_at = datetime('now', '-1 minute')")

	rows, err := f.repo.ListForUser(f.ctx, f.userID)
	if err != nil {
		t.Fatal(err)
	}
	assertTokens(t, rows, live)
}

func TestSessionsListForUserFiltersOverAgeRows(t *testing.T) {
	f := newSessionFixture(t)
	live, overAge := f.create(t), f.create(t)
	// Legacy row: over-age, but expires_at still in the future.
	f.set(t, overAge, "created_at = datetime('now', '-91 days'), expires_at = datetime('now', '+1 day')")

	rows, err := f.repo.ListForUser(f.ctx, f.userID)
	if err != nil {
		t.Fatal(err)
	}
	assertTokens(t, rows, live)
}

func TestSessionsCleanupExpired(t *testing.T) {
	f := newSessionFixture(t)
	valid, expired := f.create(t), f.create(t)
	f.set(t, expired, "expires_at = datetime('now', '-1 hour')")

	if _, err := f.repo.CleanupExpired(f.ctx); err != nil {
		t.Fatal(err)
	}
	if got := f.validate(t, valid).Kind; got != ValidateValid {
		t.Errorf("valid session kind = %v", got)
	}
	if n := f.rowCount(t, expired); n != 0 {
		t.Errorf("expired row count = %d, want 0", n)
	}
}

func TestSessionsCleanupExpiredRemovesOverAgeRows(t *testing.T) {
	f := newSessionFixture(t)
	valid, overAge := f.create(t), f.create(t)
	f.set(t, overAge, "created_at = datetime('now', '-91 days'), expires_at = datetime('now', '+1 day')")

	if _, err := f.repo.CleanupExpired(f.ctx); err != nil {
		t.Fatal(err)
	}
	if got := f.validate(t, valid).Kind; got != ValidateValid {
		t.Errorf("valid session kind = %v", got)
	}
	if n := f.rowCount(t, overAge); n != 0 {
		t.Errorf("over-age row count = %d, want 0", n)
	}
}

func TestSessionsCleanupExpiredReturnsTheNumberOfRowsRetired(t *testing.T) {
	f := newSessionFixture(t)
	f.create(t)
	expired1, expired2 := f.create(t), f.create(t)
	f.set(t, expired1, "expires_at = datetime('now', '-1 hour')")
	f.set(t, expired2, "expires_at = datetime('now', '-1 hour')")

	count, err := f.repo.CleanupExpired(f.ctx)
	if err != nil || count != 2 {
		t.Fatalf("count = %d, err = %v; want 2", count, err)
	}
	if remaining := countRows(t, f.pool, "SELECT COUNT(*) FROM sessions"); remaining != 1 {
		t.Errorf("remaining = %d, want 1", remaining)
	}
}
