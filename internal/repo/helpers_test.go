package repo

import (
	"context"
	"database/sql"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/migrations"
)

func setupTestDB(t *testing.T) *sql.DB {
	t.Helper()
	pool, err := db.OpenMemory()
	if err != nil {
		t.Fatalf("open memory db: %v", err)
	}
	t.Cleanup(func() { _ = pool.Close() })
	if err := migrations.RunForTests(context.Background(), pool); err != nil {
		t.Fatalf("migrate: %v", err)
	}
	return pool
}

// mustExec runs raw SQL, for fixtures and for bending stored timestamps.
func mustExec(t *testing.T, pool *sql.DB, query string, args ...any) {
	t.Helper()
	if _, err := pool.Exec(query, args...); err != nil {
		t.Fatalf("exec %q: %v", query, err)
	}
}

func countRows(t *testing.T, pool *sql.DB, query string, args ...any) int {
	t.Helper()
	var n int
	if err := pool.QueryRow(query, args...).Scan(&n); err != nil {
		t.Fatalf("count %q: %v", query, err)
	}
	return n
}

func createTestUser(t *testing.T, pool *sql.DB, userID string) {
	t.Helper()
	mustExec(t, pool,
		"INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?, ?, ?, ?, datetime('now'))",
		userID, "user_"+userID, "hash", "user")
}

func createTestExercise(t *testing.T, pool *sql.DB, exerciseID, userID string) {
	t.Helper()
	mustExec(t, pool,
		"INSERT INTO exercises (id, name, category, user_id) VALUES (?, ?, ?, ?)",
		exerciseID, "Test Exercise", "chest", userID)
}

// createRepoUser creates a user through the repository.
func createRepoUser(t *testing.T, pool *sql.DB, username string) models.User {
	t.Helper()
	user, err := NewUsers(pool).Create(context.Background(),
		NewUser{Username: username, PasswordHash: "hash", Role: models.RoleUser})
	if err != nil {
		t.Fatalf("create user: %v", err)
	}
	return user
}

func day(year int, month time.Month, d int) time.Time {
	return time.Date(year, month, d, 0, 0, 0, 0, time.UTC)
}

func ptr[T any](v T) *T { return &v }
