package repo

import (
	"context"
	"database/sql"
	"log/slog"
	"slices"
	"testing"
	"time"

	"github.com/henry40408/liftlog/migrations"
)

const (
	rustAdminID   = "4e85b172-fa25-4a95-81b6-8df5f8911cf7"
	rustWorkoutID = "5ec1073b-9cbf-45bd-86cc-80ae5afcb356"
	rustSquatID   = "8227bbdc-efec-45d3-b9fb-c15ed75f0df0"
	rustToken     = "68c9c4b1-805f-4561-abcd-96e40ae2f67d"
)

func TestRustCreatedDatabaseReadsThroughRepositories(t *testing.T) {
	pool := openRustDB(t)
	ctx := context.Background()

	admin, err := NewUsers(pool).FindByUsername(ctx, "admin")
	if err != nil {
		t.Fatal(err)
	}
	if admin.ID != rustAdminID || !admin.Role.IsAdmin() || admin.Timezone != nil {
		t.Errorf("admin = %+v", admin)
	}
	wantCreated := time.Date(2026, 11, 16, 16, 0, 0, 0, time.UTC)
	if admin.CreatedAt.Before(wantCreated) || admin.CreatedAt.After(wantCreated.Add(2*time.Hour)) {
		t.Errorf("admin created_at = %v", admin.CreatedAt)
	}

	workouts := NewWorkouts(pool)
	session, err := workouts.FindOwnedSession(ctx, rustWorkoutID, rustAdminID)
	if err != nil {
		t.Fatal(err)
	}
	if !session.Date.Equal(day(2026, 11, 10)) || session.Notes == nil || *session.Notes != "First session" {
		t.Errorf("session = %+v", session)
	}
	if session.ShareToken == nil || *session.ShareToken != rustToken || session.ShareExpiresAt == nil {
		t.Fatalf("share fields = %v, %v", session.ShareToken, session.ShareExpiresAt)
	}
	wantExpiry := time.Date(2026, 12, 16, 16, 38, 56, 97583000, time.UTC).Truncate(time.Second)
	if !session.ShareExpiresAt.Truncate(time.Second).Equal(wantExpiry) {
		t.Errorf("share_expires_at = %v", session.ShareExpiresAt)
	}

	logs, err := workouts.FindLogsBySessionWithPR(ctx, rustWorkoutID, rustAdminID, time.Now().Add(-30*24*time.Hour))
	if err != nil || len(logs) != 3 {
		t.Fatalf("logs = %d, %v", len(logs), err)
	}
}

func TestRustCreatedDatabaseSessionsAndMetrics(t *testing.T) {
	pool := openRustDB(t)
	ctx := context.Background()

	rows, err := NewSessions(pool).ListForUser(ctx, rustAdminID)
	if err != nil {
		t.Fatal(err)
	}
	for _, row := range rows {
		if row.CreatedAt.IsZero() || row.LastTouchedAt.IsZero() {
			t.Errorf("unparsed session times: %+v", row)
		}
	}

	metrics, err := NewWorkouts(pool).SessionMetricsForExercise(ctx, rustAdminID, rustSquatID)
	if err != nil || len(metrics) != 1 {
		t.Fatalf("metrics = %+v, %v", metrics, err)
	}
	if m := metrics[0]; !m.Date.Equal(day(2026, 11, 10)) || m.TopWeight != 102.5 || m.TopReps != 5 || m.Volume != 1012.5 {
		t.Errorf("metric = %+v", m)
	}
}

// A database the Rust binary migrated must start under the Go build without
// touching what Rust recorded: every row stays, with its original applied_at.
func TestMigrationsRunKeepsTheRustCreatedTrackingRows(t *testing.T) {
	pool := openRustDB(t)

	before := trackedMigrations(t, pool)
	if len(before) == 0 {
		t.Fatal("the Rust-created database has no tracking rows")
	}
	if err := migrations.Run(context.Background(), pool, slog.New(slog.DiscardHandler)); err != nil {
		t.Fatal(err)
	}
	// A migration added later is appended; the captured rows keep their place.
	after := trackedMigrations(t, pool)
	if len(after) < len(before) || !slices.Equal(after[:len(before)], before) {
		t.Errorf("_migrations changed:\nbefore %q\nafter  %q", before, after)
	}
}

func trackedMigrations(t *testing.T, pool *sql.DB) []string {
	t.Helper()
	rows, err := pool.Query("SELECT name || ' ' || applied_at FROM _migrations ORDER BY rowid")
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = rows.Close() }()
	var tracked []string
	for rows.Next() {
		var row string
		if err := rows.Scan(&row); err != nil {
			t.Fatal(err)
		}
		tracked = append(tracked, row)
	}
	if err := rows.Err(); err != nil {
		t.Fatal(err)
	}
	return tracked
}
