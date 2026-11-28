package migrations

import (
	"database/sql"
	"slices"
	"testing"
)

// moreOwnerRows adds to pre04Rows a second user who logged the same seeded
// exercise, a user without sets and a seeded exercise nobody logged.
const moreOwnerRows = `
INSERT INTO users VALUES
    ('u2', 'second', 'hash', '2026-02-01 00:00:00', 'user', NULL),
    ('u3', 'idle', 'hash', '2026-02-01 00:00:00', 'user', NULL);
INSERT INTO exercises VALUES ('ex-deadlift', 'Deadlift', 'back', 'back', 'barbell', 1, NULL);
INSERT INTO workout_sessions VALUES ('w2', 'u2', '2026-02-02', NULL, '2026-02-02 00:00:00', NULL, NULL);
INSERT INTO workout_logs VALUES
    ('l3', 'w2', 'ex-squat', 1, 5, 80, NULL, 0, '2026-02-02 00:00:00'),
    ('l4', 'w2', 'ex-squat', 2, 5, 80, NULL, 0, '2026-02-02 00:00:00');
`

// orphanRows is a set of a seeded exercise in a session whose user is gone.
const orphanRows = `
PRAGMA foreign_keys = OFF;
INSERT INTO workout_sessions VALUES ('w9', 'gone', '2026-02-03', NULL, '2026-02-03 00:00:00', NULL, NULL);
INSERT INTO workout_logs VALUES ('l9', 'w9', 'ex-squat', 1, 5, 60, NULL, 0, '2026-02-03 00:00:00');
PRAGMA foreign_keys = ON;
`

func pre04DatabaseWithMoreOwners(t *testing.T) *sql.DB {
	t.Helper()
	pool := openMemory(t)
	createPre04Database(t, pool)
	mustExec(t, pool, moreOwnerRows)
	return pool
}

func exerciseOwners(t *testing.T, pool *sql.DB) []string {
	t.Helper()
	return queryStrings(t, pool,
		"SELECT concat_ws('|', id, name, category, ifnull(user_id, '-')) FROM exercises ORDER BY 1")
}

func loggedExercises(t *testing.T, pool *sql.DB) []string {
	t.Helper()
	return queryStrings(t, pool, "SELECT id || '|' || exercise_id FROM workout_logs ORDER BY 1")
}

// untouchedRows is what moving sets between exercises must not change.
func untouchedRows(t *testing.T, pool *sql.DB) []string {
	t.Helper()
	return queryStrings(t, pool, `
		SELECT concat_ws('|', 'log', id, session_id, set_number, reps, weight, rpe, is_pr, created_at)
		FROM workout_logs
		UNION ALL
		SELECT concat_ws('|', 'record', id, user_id, exercise_id, record_type, value, achieved_at) FROM personal_records
		ORDER BY 1`)
}

func TestEachUserGetsACopyOfTheOwnerlessExercisesTheyLogged(t *testing.T) {
	pool := pre04DatabaseWithMoreOwners(t)
	untouched := untouchedRows(t, pool)

	noError(t, Run(ctx, pool, discard))

	wantOwners := []string{
		"e1|Own squat|legs|u1",
		"ex-deadlift|Deadlift|back|-",
		"ex-squat-u1|Squat|legs|u1",
		"ex-squat-u2|Squat|legs|u2",
		"ex-squat|Squat|legs|-",
	}
	if got := exerciseOwners(t, pool); !slices.Equal(got, wantOwners) {
		t.Errorf("exercises = %q, want %q", got, wantOwners)
	}
	wantLogs := []string{"l1|e1", "l2|ex-squat-u1", "l3|ex-squat-u2", "l4|ex-squat-u2"}
	if got := loggedExercises(t, pool); !slices.Equal(got, wantLogs) {
		t.Errorf("logs = %q, want %q", got, wantLogs)
	}
	if got := untouchedRows(t, pool); !slices.Equal(got, untouched) {
		t.Errorf("other data changed\n got: %q\nwant: %q", got, untouched)
	}
	if got := queryStrings(t, pool, "SELECT count(*) FROM pragma_foreign_key_check"); !slices.Equal(got, []string{"0"}) {
		t.Errorf("foreign key violations = %q", got)
	}
}

// A copy gives its user an exercise that logged sets restrict, so the cascade
// from the user has to remove the sets before the exercise.
func TestAUserWithACopiedExerciseCanBeDeleted(t *testing.T) {
	pool := pre04DatabaseWithMoreOwners(t)
	noError(t, Run(ctx, pool, discard))
	if got := loggedExercises(t, pool); !slices.Contains(got, "l3|ex-squat-u2") {
		t.Fatalf("logs = %q, want l3 on a copy; the deletion would prove nothing", got)
	}

	mustExec(t, pool, "DELETE FROM users WHERE id = 'u2'")
	mustExec(t, pool, "DELETE FROM users WHERE id = 'u1'")

	want := []string{"ex-deadlift|Deadlift|back|-", "ex-squat|Squat|legs|-"}
	if got := exerciseOwners(t, pool); !slices.Equal(got, want) {
		t.Errorf("exercises = %q, want %q", got, want)
	}
}

// A copy without an owner would break a foreign key and stop the upgrade.
func TestSetsWithoutAUserKeepTheOwnerlessExercise(t *testing.T) {
	pool := pre04DatabaseWithMoreOwners(t)
	mustExec(t, pool, orphanRows)

	noError(t, Run(ctx, pool, discard))

	if got := loggedExercises(t, pool); !slices.Contains(got, "l9|ex-squat") {
		t.Errorf("logs = %q, want l9 left on ex-squat", got)
	}
}
