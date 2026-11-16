package repo

import (
	"errors"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/db"
)

func TestShareSetToken(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	if session.ShareToken != nil {
		t.Fatal("new session already has a token")
	}

	token, err := f.repo.SetShareToken(f.ctx, session.ID, "user1", nil)
	if err != nil || token == "" {
		t.Fatalf("SetShareToken = %q, %v", token, err)
	}
	found, _ := f.repo.FindSessionByID(f.ctx, session.ID)
	if found.ShareToken == nil || *found.ShareToken != token || found.ShareExpiresAt != nil {
		t.Errorf("found %+v", found)
	}
}

func TestShareSetTokenWithTTLStoresExpiry(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)

	ttl := 24 * time.Hour
	if _, err := f.repo.SetShareToken(f.ctx, session.ID, "user1", &ttl); err != nil {
		t.Fatal(err)
	}
	found, _ := f.repo.FindSessionByID(f.ctx, session.ID)
	if found.ShareExpiresAt == nil || !within(*found.ShareExpiresAt, time.Now().Add(ttl), 5*time.Second) {
		t.Errorf("expires_at = %v, want ~now+24h", found.ShareExpiresAt)
	}
}

func TestShareSetTokenWrongUser(t *testing.T) {
	f := newWorkoutFixture(t, "user1", "user2")
	session := f.session(t, "user1", day(2024, 1, 15), nil)

	if _, err := f.repo.SetShareToken(f.ctx, session.ID, "user2", nil); !errors.Is(err, ErrNotFound) {
		t.Fatalf("err = %v, want ErrNotFound", err)
	}
}

func TestShareRevokeToken(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	ttl := time.Hour
	if _, err := f.repo.SetShareToken(f.ctx, session.ID, "user1", &ttl); err != nil {
		t.Fatal(err)
	}

	revoked, err := f.repo.RevokeShareToken(f.ctx, session.ID, "user1")
	if err != nil || !revoked {
		t.Fatalf("RevokeShareToken = %v, %v", revoked, err)
	}
	found, _ := f.repo.FindSessionByID(f.ctx, session.ID)
	if found.ShareToken != nil || found.ShareExpiresAt != nil {
		t.Errorf("token or expiry survived revoke: %+v", found)
	}
}

func TestShareRevokeTokenWrongUser(t *testing.T) {
	f := newWorkoutFixture(t, "user1", "user2")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	if _, err := f.repo.SetShareToken(f.ctx, session.ID, "user1", nil); err != nil {
		t.Fatal(err)
	}

	revoked, err := f.repo.RevokeShareToken(f.ctx, session.ID, "user2")
	if err != nil || revoked {
		t.Fatalf("RevokeShareToken = %v, %v; want false, nil", revoked, err)
	}
	if found, _ := f.repo.FindSessionByID(f.ctx, session.ID); found.ShareToken == nil {
		t.Error("another user revoked the token")
	}
}

func TestShareFindSessionByToken(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	token, _ := f.repo.SetShareToken(f.ctx, session.ID, "user1", nil)

	found, err := f.repo.FindSessionByShareToken(f.ctx, token)
	if err != nil || found.ID != session.ID {
		t.Fatalf("found %+v, %v", found, err)
	}
	if _, err := f.repo.FindSessionByShareToken(f.ctx, "nonexistent-token"); !errors.Is(err, ErrNotFound) {
		t.Errorf("unknown token: err = %v, want ErrNotFound", err)
	}
}

// Expired and nonexistent tokens must be indistinguishable.
func TestShareFindSessionByTokenFiltersExpiry(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	ttl := time.Hour
	token, _ := f.repo.SetShareToken(f.ctx, session.ID, "user1", &ttl)

	if _, err := f.repo.FindSessionByShareToken(f.ctx, token); err != nil {
		t.Fatalf("live token: %v", err)
	}
	mustExec(t, f.pool, "UPDATE workout_sessions SET share_expires_at = ? WHERE id = ?",
		db.FormatTime(time.Now().Add(-time.Minute)), session.ID)
	if _, err := f.repo.FindSessionByShareToken(f.ctx, token); !errors.Is(err, ErrNotFound) {
		t.Errorf("expired token: err = %v, want ErrNotFound", err)
	}
}

func TestShareCleanupExpiredTokens(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	live := f.session(t, "user1", day(2024, 1, 15), nil)
	expired := f.session(t, "user1", day(2024, 1, 16), nil)
	forever := f.session(t, "user1", day(2024, 1, 17), nil)
	ttl := time.Hour
	for _, session := range []string{live.ID, expired.ID} {
		if _, err := f.repo.SetShareToken(f.ctx, session, "user1", &ttl); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := f.repo.SetShareToken(f.ctx, forever.ID, "user1", nil); err != nil {
		t.Fatal(err)
	}
	mustExec(t, f.pool, "UPDATE workout_sessions SET share_expires_at = ? WHERE id = ?",
		db.FormatTime(time.Now().Add(-time.Minute)), expired.ID)

	count, err := f.repo.CleanupExpiredShareTokens(f.ctx)
	if err != nil || count != 1 {
		t.Fatalf("count = %d, %v; want 1", count, err)
	}
	for id, wantToken := range map[string]bool{live.ID: true, expired.ID: false, forever.ID: true} {
		found, _ := f.repo.FindSessionByID(f.ctx, id)
		if (found.ShareToken != nil) != wantToken {
			t.Errorf("session %s token present = %v, want %v", id, found.ShareToken != nil, wantToken)
		}
	}
}

func TestShareFindLogsBySessionForShareSkipsPRCalculation(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	f.log(t, session.ID, "ex-bench-press", 1, 10, 100, nil)

	logs, err := f.repo.FindLogsBySessionForShare(f.ctx, session.ID)
	if err != nil || len(logs) != 1 {
		t.Fatalf("got %d logs, %v", len(logs), err)
	}
	if logs[0].IsPR || logs[0].IsRecentPR {
		t.Error("share view must not compute PRs")
	}
}

func TestShareReshareGeneratesNewToken(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)

	token1, _ := f.repo.SetShareToken(f.ctx, session.ID, "user1", nil)
	if _, err := f.repo.RevokeShareToken(f.ctx, session.ID, "user1"); err != nil {
		t.Fatal(err)
	}
	token2, _ := f.repo.SetShareToken(f.ctx, session.ID, "user1", nil)
	if token1 == token2 {
		t.Error("re-share reused the token")
	}
}
