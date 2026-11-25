package repo

import (
	"context"
	"database/sql"
	"path/filepath"
	"testing"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/migrations"
)

// rustMigrationsTable is the `_migrations` statement as the Rust binary stored
// it in sqlite_master.
const rustMigrationsTable = `CREATE TABLE _migrations (
            name TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )`

// rustCreatedRows are the rows a Rust build (commit 19c79d8, all 11 migrations
// applied) left behind on 2026-11-16 around 16:38 UTC, after a run through the
// HTTP API of an empty server: setup of admin, a login and logout by bob (no
// session row), two exercises, one workout with three logs, and a share link.
// Every value is as captured: chrono + rusqlite wrote timestamps as
// `YYYY-MM-DD HH:MM:SS.ffffff+00:00` (six fractional digits, because macOS has
// microsecond precision; Linux writes nine, a whole second none), dates as
// `YYYY-MM-DD`, and weights as REAL (`100.0`). The Rust binary recorded every
// migration with the same `applied_at`. The session expired on 2026-11-23 and
// the share link expires on 2026-12-16, so tests must check how the rows are
// read, not whether they are still valid. The capture passwords are not
// recorded; the users hold the argon2id hashes the Rust server stored.
var rustCreatedRows = []string{
	`INSERT INTO _migrations (name, applied_at) VALUES
		('001_create_users.sql', '2026-11-16 16:38:53'),
		('002_create_exercises.sql', '2026-11-16 16:38:53'),
		('003_create_workout_sessions.sql', '2026-11-16 16:38:53'),
		('004_create_workout_logs.sql', '2026-11-16 16:38:53'),
		('007_add_user_role.sql', '2026-11-16 16:38:53'),
		('008_create_sessions.sql', '2026-11-16 16:38:53'),
		('009_add_workout_share_token.sql', '2026-11-16 16:38:53'),
		('010_rebuild_sessions_with_last_touched_at.sql', '2026-11-16 16:38:53'),
		('011_cleanup_orphan_rows.sql', '2026-11-16 16:38:53'),
		('012_add_workout_share_expires_at.sql', '2026-11-16 16:38:53'),
		('013_add_user_timezone.sql', '2026-11-16 16:38:53')`,
	`INSERT INTO users (id, username, password_hash, created_at, role, timezone) VALUES
		('4e85b172-fa25-4a95-81b6-8df5f8911cf7', 'admin', '$argon2id$v=19$m=19456,t=2,p=1$VaLQAf6ggeSfTM07ktvfVQ$zyBbSo7gIrbZeivs6lxwcbllkYgXzsBKxSqPVvj8Pyo', '2026-11-16 16:38:55.588074+00:00', 'admin', NULL),
		('69da6614-050d-46f7-8630-68cc4f4d5230', 'bob', '$argon2id$v=19$m=19456,t=2,p=1$aEXK+knepjyHpRlSZpwyvg$eJUTQ5pXJ5tUPHwfW0wnsgRPIPw9fAYXylHmMloulPQ', '2026-11-16 16:38:56.111763+00:00', 'user', NULL)`,
	`INSERT INTO sessions (token, user_id, created_at, expires_at, last_touched_at) VALUES
		('547faca9-954c-4a03-ae83-56241b5e56c9', '4e85b172-fa25-4a95-81b6-8df5f8911cf7',
		 '2026-11-16 16:38:55.725186+00:00', '2026-11-23 16:38:55.725186+00:00', '2026-11-16 16:38:55.725186+00:00')`,
	`INSERT INTO exercises (id, name, category, user_id) VALUES
		('8227bbdc-efec-45d3-b9fb-c15ed75f0df0', 'Squat', 'legs', '4e85b172-fa25-4a95-81b6-8df5f8911cf7'),
		('af6b51cc-a27f-4cd0-a849-5931f64a2d8a', 'Bench Press', 'chest', '4e85b172-fa25-4a95-81b6-8df5f8911cf7')`,
	`INSERT INTO workout_sessions (id, user_id, date, notes, created_at, share_token, share_expires_at) VALUES
		('5ec1073b-9cbf-45bd-86cc-80ae5afcb356', '4e85b172-fa25-4a95-81b6-8df5f8911cf7', '2026-11-10', 'First session',
		 '2026-11-16 16:38:56.026458+00:00', '68c9c4b1-805f-4561-abcd-96e40ae2f67d', '2026-12-16 16:38:56.097583+00:00')`,
	`INSERT INTO workout_logs (id, session_id, exercise_id, set_number, reps, weight, rpe, created_at) VALUES
		('62b112b7-59a1-424f-8ccc-7d8060e17517', '5ec1073b-9cbf-45bd-86cc-80ae5afcb356', '8227bbdc-efec-45d3-b9fb-c15ed75f0df0',
		 1, 5, 100.0, 8, '2026-11-16 16:38:56.051377+00:00'),
		('9e8f36cf-b84a-4892-9107-84acfe902459', '5ec1073b-9cbf-45bd-86cc-80ae5afcb356', '8227bbdc-efec-45d3-b9fb-c15ed75f0df0',
		 2, 5, 102.5, NULL, '2026-11-16 16:38:56.067682+00:00'),
		('94bf6183-bf8c-4c7b-8ec5-2de5b8cc87b1', '5ec1073b-9cbf-45bd-86cc-80ae5afcb356', 'af6b51cc-a27f-4cd0-a849-5931f64a2d8a',
		 1, 8, 60.0, 7, '2026-11-16 16:38:56.083179+00:00')`,
}

// openRustDB rebuilds the database the Rust binary left behind: the schema from
// the embedded migrations, the `_migrations` table as the Rust binary wrote it,
// and the captured rows, so that timestamps, dates and weights are in the
// format chrono + rusqlite produced.
func openRustDB(t *testing.T) *sql.DB {
	t.Helper()
	pool, err := db.Open("sqlite:" + filepath.Join(t.TempDir(), "liftlog.sqlite3"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = pool.Close() })

	// First, as in the capture: sqlite_master lists the tables in creation order.
	mustExec(t, pool, rustMigrationsTable)
	if err := migrations.RunForTests(context.Background(), pool); err != nil {
		t.Fatal(err)
	}
	for _, insert := range rustCreatedRows {
		mustExec(t, pool, insert)
	}
	return pool
}
