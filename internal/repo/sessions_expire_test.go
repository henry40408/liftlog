package repo

import (
	"strings"
	"testing"
)

// Deleting a stale row is part of validating it; when the delete fails the
// caller must see the error, not an "expired" verdict for a row that survived.
func TestSessionsValidateReportsAFailedDeleteOfAnExpiredRow(t *testing.T) {
	f := newSessionFixture(t)
	token := f.create(t)
	f.set(t, token, "expires_at = datetime('now', '-1 hour')")
	mustExec(t, f.pool, `CREATE TRIGGER block_session_delete BEFORE DELETE ON sessions
		BEGIN SELECT RAISE(ABORT, 'forced failure'); END`)

	_, err := f.repo.ValidateAndTouch(f.ctx, token)
	if err == nil || !strings.Contains(err.Error(), "forced failure") {
		t.Fatalf("ValidateAndTouch err = %v, want the forced delete failure", err)
	}
	if n := f.rowCount(t, token); n != 1 {
		t.Errorf("row count = %d, want the row to survive", n)
	}
}
