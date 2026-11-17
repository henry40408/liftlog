package main

import (
	"context"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/migrations"
)

func TestSweepOnceClearsExpiredShareTokensOnly(t *testing.T) {
	ctx := context.Background()
	pool, err := db.OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = pool.Close() })
	if err := migrations.RunForTests(ctx, pool); err != nil {
		t.Fatal(err)
	}
	past := db.FormatTime(time.Now().Add(-time.Hour))
	future := db.FormatTime(time.Now().Add(time.Hour))
	mustExec(t, pool, `INSERT INTO users (id, username, password_hash, created_at) VALUES ('u1', 'owner', 'x', ?)`, past)
	insert := `INSERT INTO workout_sessions (id, user_id, date, share_token, share_expires_at) VALUES (?, 'u1', '2026-01-01', ?, ?)`
	mustExec(t, pool, insert, "expired", "tok-expired", past)
	mustExec(t, pool, insert, "live", "tok-live", future)
	mustExec(t, pool, insert, "forever", "tok-forever", nil)

	logger, sink := captureLogger()
	newSweeper(pool, logger).sweepOnce(ctx)

	requireContains(t, sink.String(), "cleared expired workout share tokens", "count=1")
	remaining := map[string]bool{}
	rows, err := pool.Query(`SELECT id FROM workout_sessions WHERE share_token IS NOT NULL`)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = rows.Close() }()
	for rows.Next() {
		var id string
		if err := rows.Scan(&id); err != nil {
			t.Fatal(err)
		}
		remaining[id] = true
	}
	if remaining["expired"] || !remaining["live"] || !remaining["forever"] {
		t.Errorf("shared workouts after the sweep = %v, want live and forever only", remaining)
	}
}

func TestSweepOnceReportsBothFailuresIndependently(t *testing.T) {
	pool, err := db.OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	// The pool is closed on purpose, so both cleanups fail.
	if err := pool.Close(); err != nil {
		t.Fatal(err)
	}

	logger, sink := captureLogger()
	newSweeper(pool, logger).sweepOnce(context.Background())

	requireContains(t, sink.String(),
		"session cleanup_expired failed",
		"workout cleanup_expired_share_tokens failed")
}
