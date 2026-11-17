package db

import (
	"context"
	"strings"
	"testing"
)

func TestCheckpointReportsAFailureWithContext(t *testing.T) {
	pool, err := OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	// The pool is closed on purpose, so the pragma cannot run.
	if err := pool.Close(); err != nil {
		t.Fatal(err)
	}

	err = Checkpoint(context.Background(), pool)
	if err == nil || !strings.Contains(err.Error(), "wal checkpoint") {
		t.Fatalf("err = %v, want it to start with the wal checkpoint context", err)
	}
}
