package main

import (
	"bytes"
	"context"
	"database/sql"
	"log/slog"
	"strings"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/migrations"
)

func TestSweepOnceRetiresExpiredSessionsWithAnAuditEvent(t *testing.T) {
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
	mustExec(t, pool, `INSERT INTO users (id, username, password_hash, created_at) VALUES ('u1', 'owner', 'x', ?)`, past)
	mustExec(t, pool, `INSERT INTO sessions (token, user_id, created_at, expires_at, last_touched_at) VALUES ('tok', 'u1', ?, ?, ?)`, past, past, past)

	var logs bytes.Buffer
	newSweeper(pool, slog.New(slog.NewTextHandler(&logs, nil))).sweepOnce(ctx)

	for _, want := range []string{"target=liftlog::audit", "event=session.expired", "reason=sweep", "count=1"} {
		if !strings.Contains(logs.String(), want) {
			t.Errorf("log lacks %q:\n%s", want, logs.String())
		}
	}
}

func TestRunSweepsOnceAtStartup(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	pool, err := db.OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = pool.Close() })
	if err := migrations.RunForTests(ctx, pool); err != nil {
		t.Fatal(err)
	}
	past := db.FormatTime(time.Now().Add(-time.Hour))
	mustExec(t, pool, `INSERT INTO users (id, username, password_hash, created_at) VALUES ('u1', 'owner', 'x', ?)`, past)
	mustExec(t, pool, `INSERT INTO sessions (token, user_id, created_at, expires_at, last_touched_at) VALUES ('tok', 'u1', ?, ?, ?)`, past, past, past)

	// The interval is far beyond the test, so only the startup pass can sweep.
	done := make(chan struct{})
	go func() {
		defer close(done)
		newSweeper(pool, slog.New(slog.DiscardHandler)).run(ctx, time.Hour)
	}()
	deadline := time.After(5 * time.Second)
	for {
		var remaining int
		if err := pool.QueryRow(`SELECT COUNT(*) FROM sessions`).Scan(&remaining); err != nil {
			t.Fatal(err)
		}
		if remaining == 0 {
			break
		}
		select {
		case <-deadline:
			t.Fatal("the expired session survived the startup sweep")
		case <-time.After(10 * time.Millisecond):
		}
	}
	cancel()
	<-done
}

func TestStopWaitsForTheSweepAndMayRepeat(t *testing.T) {
	pool, err := db.OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = pool.Close() })
	stop := startExpirySweep(pool, slog.New(slog.DiscardHandler))
	stop()
	stop()
}

func mustExec(t *testing.T, pool *sql.DB, query string, args ...any) {
	t.Helper()
	if _, err := pool.Exec(query, args...); err != nil {
		t.Fatal(err)
	}
}
