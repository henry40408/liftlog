package migrations

import (
	"context"
	"database/sql"
	"io"
	"log/slog"
	"testing"

	"github.com/henry40408/liftlog/internal/db"
)

var (
	ctx     = context.Background()
	discard = slog.New(slog.NewTextHandler(io.Discard, nil))
)

func openMemory(t *testing.T) *sql.DB {
	t.Helper()
	pool, err := db.OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = pool.Close() })
	return pool
}

func mustExec(t *testing.T, pool *sql.DB, query string, args ...any) {
	t.Helper()
	if _, err := pool.ExecContext(ctx, query, args...); err != nil {
		t.Fatalf("%s: %v", query, err)
	}
}

func count(t *testing.T, pool *sql.DB, query string, args ...any) int {
	t.Helper()
	var n int
	if err := pool.QueryRowContext(ctx, query, args...).Scan(&n); err != nil {
		t.Fatalf("%s: %v", query, err)
	}
	return n
}

func mustAll(t *testing.T) []migration {
	t.Helper()
	migrations, err := all()
	if err != nil {
		t.Fatal(err)
	}
	return migrations
}

func indexOf(t *testing.T, migrations []migration, name string) int {
	t.Helper()
	for i, m := range migrations {
		if m.name == name {
			return i
		}
	}
	t.Fatalf("%s is not registered", name)
	return -1
}

func TestAllAreInAscendingOrder(t *testing.T) {
	migrations := mustAll(t)
	if len(migrations) != 11 {
		t.Errorf("got %d migrations, want 11", len(migrations))
	}
	for i := 1; i < len(migrations); i++ {
		if migrations[i-1].name >= migrations[i].name {
			t.Errorf("%s is not before %s", migrations[i-1].name, migrations[i].name)
		}
	}
}

func TestRunCreatesTrackingTableAndRecordsEachMigration(t *testing.T) {
	pool := openMemory(t)
	if err := Run(ctx, pool, discard); err != nil {
		t.Fatal(err)
	}
	if got, want := count(t, pool, "SELECT COUNT(*) FROM _migrations"), len(mustAll(t)); got != want {
		t.Errorf("recorded %d migrations, want %d", got, want)
	}
}

func TestRunIsIdempotent(t *testing.T) {
	pool := openMemory(t)
	for range 2 {
		if err := Run(ctx, pool, discard); err != nil {
			t.Fatal(err)
		}
	}
	if got, want := count(t, pool, "SELECT COUNT(*) FROM _migrations"), len(mustAll(t)); got != want {
		t.Errorf("recorded %d migrations, want %d", got, want)
	}
}

func TestForeignKeyCheckIsCleanAfterMigrations(t *testing.T) {
	pool := openMemory(t)
	if err := Run(ctx, pool, discard); err != nil {
		t.Fatal(err)
	}
	if got := count(t, pool, "SELECT COUNT(*) FROM pragma_foreign_key_check"); got != 0 {
		t.Errorf("%d foreign key violations", got)
	}
}

func TestForeignKeysAreOnAfterRun(t *testing.T) {
	pool := openMemory(t)
	if err := Run(ctx, pool, discard); err != nil {
		t.Fatal(err)
	}
	if got := count(t, pool, "SELECT foreign_keys FROM pragma_foreign_keys"); got != 1 {
		t.Errorf("foreign_keys = %d, want 1", got)
	}
}

func TestRunForTestsAppliesEverythingAndKeepsForeignKeysOn(t *testing.T) {
	pool := openMemory(t)
	if err := RunForTests(ctx, pool); err != nil {
		t.Fatal(err)
	}
	if got := count(t, pool, "SELECT COUNT(*) FROM sqlite_master WHERE name = 'workout_logs'"); got != 1 {
		t.Errorf("workout_logs table missing")
	}
	if got := count(t, pool, "SELECT foreign_keys FROM pragma_foreign_keys"); got != 1 {
		t.Errorf("foreign_keys = %d, want 1", got)
	}
}

func TestCleanupMigrationRemovesPreexistingOrphans(t *testing.T) {
	pool := openMemory(t)
	migrations := mustAll(t)
	cleanup := indexOf(t, migrations, "011_cleanup_orphan_rows.sql")
	for _, m := range migrations[:cleanup] {
		mustExec(t, pool, m.sql)
	}
	// Explicit, so the orphan inserts do not rely on 010's side effect.
	mustExec(t, pool, "PRAGMA foreign_keys = OFF")

	mustExec(t, pool, `INSERT INTO users (id, username, password_hash, created_at)
		VALUES ('u1', 'u1', 'hash', datetime('now'))`)
	// Orphan session: user_id matches no users row.
	mustExec(t, pool, `INSERT INTO sessions (token, user_id, created_at, expires_at, last_touched_at)
		VALUES ('tok1', 'ghost-user', datetime('now'), datetime('now', '+1 day'), datetime('now'))`)
	// Orphan workout_log: session_id matches no workout_sessions row.
	mustExec(t, pool, `INSERT INTO exercises (id, name, category, user_id)
		VALUES ('ex1', 'Squat', 'legs', 'u1')`)
	mustExec(t, pool, `INSERT INTO workout_logs (id, session_id, exercise_id, set_number, reps, weight)
		VALUES ('log1', 'ghost-session', 'ex1', 1, 5, 100.0)`)

	mustExec(t, pool, migrations[cleanup].sql)

	if got := count(t, pool, "SELECT COUNT(*) FROM sessions WHERE token = 'tok1'"); got != 0 {
		t.Error("orphan session should be removed")
	}
	if got := count(t, pool, "SELECT COUNT(*) FROM workout_logs WHERE id = 'log1'"); got != 0 {
		t.Error("orphan workout log should be removed")
	}
}

// An existing database skips 010, so 011 must work without 010 having just
// turned foreign_keys off.
func TestCleanupMigrationClearsOrphansOnADatabaseAlreadyAt010(t *testing.T) {
	pool := openMemory(t)
	migrations := mustAll(t)
	through010 := migrations[:indexOf(t, migrations, "010_rebuild_sessions_with_last_touched_at.sql")+1]

	mustExec(t, pool, `CREATE TABLE IF NOT EXISTS _migrations (
		name TEXT PRIMARY KEY,
		applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)`)
	for _, m := range through010 {
		mustExec(t, pool, m.sql)
		// Recorded as applied, so Run starts at 011.
		mustExec(t, pool, "INSERT INTO _migrations (name) VALUES (?)", m.name)
	}
	// Off only to insert the orphans below.
	mustExec(t, pool, "PRAGMA foreign_keys = OFF")

	mustExec(t, pool, `INSERT INTO users (id, username, password_hash, created_at)
		VALUES ('u1', 'u1', 'hash', datetime('now'))`)
	mustExec(t, pool, `INSERT INTO sessions (token, user_id, created_at, expires_at, last_touched_at)
		VALUES ('orphan-tok', 'ghost-user', datetime('now'), datetime('now', '+1 day'), datetime('now'))`)
	mustExec(t, pool, `INSERT INTO workout_sessions (id, user_id, date, created_at)
		VALUES ('orphan-ws', 'ghost-user2', '2024-01-01', datetime('now'))`)
	mustExec(t, pool, `INSERT INTO exercises (id, name, category, user_id)
		VALUES ('orphan-ex', 'Squat', 'legs', 'ghost-user3')`)
	// A log under a real session but the orphan exercise: orphaned only once
	// the exercise is deleted, so 011 must delete parents first.
	mustExec(t, pool, `INSERT INTO workout_sessions (id, user_id, date, created_at)
		VALUES ('real-ws', 'u1', '2024-01-02', datetime('now'))`)
	mustExec(t, pool, `INSERT INTO workout_logs (id, session_id, exercise_id, set_number, reps, weight)
		VALUES ('log1', 'real-ws', 'orphan-ex', 1, 5, 100.0)`)

	if err := Run(ctx, pool, discard); err != nil {
		t.Fatalf("Run must not abort on pre-existing orphans: %v", err)
	}

	if got := count(t, pool, "SELECT COUNT(*) FROM pragma_foreign_key_check"); got != 0 {
		t.Errorf("%d foreign key violations remain", got)
	}
	for _, row := range []struct{ table, idColumn, id string }{
		{"sessions", "token", "orphan-tok"},
		{"workout_sessions", "id", "orphan-ws"},
		{"exercises", "id", "orphan-ex"},
		{"workout_logs", "id", "log1"},
	} {
		query := "SELECT COUNT(*) FROM " + row.table + " WHERE " + row.idColumn + " = ?"
		if got := count(t, pool, query, row.id); got != 0 {
			t.Errorf("%s row %s should have been cleaned up", row.table, row.id)
		}
	}
}
