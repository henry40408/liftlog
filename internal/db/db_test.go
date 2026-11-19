package db

import (
	"context"
	"database/sql"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func pragmaInt(t *testing.T, q interface {
	QueryRowContext(context.Context, string, ...any) *sql.Row
}, pragma string) int {
	t.Helper()
	var value int
	if err := q.QueryRowContext(context.Background(), "PRAGMA "+pragma).Scan(&value); err != nil {
		t.Fatalf("PRAGMA %s: %v", pragma, err)
	}
	return value
}

func openTempFile(t *testing.T) (*sql.DB, string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "liftlog.sqlite3")
	pool, err := Open("sqlite:" + path)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = pool.Close() })
	return pool, path
}

func TestOpenMemoryURL(t *testing.T) {
	pool, err := Open("sqlite::memory:")
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = pool.Close() }()
	if got := pragmaInt(t, pool, "foreign_keys"); got != 1 {
		t.Errorf("foreign_keys = %d", got)
	}
	var one int
	if err := pool.QueryRow("SELECT 1").Scan(&one); err != nil || one != 1 {
		t.Errorf("SELECT 1 = %d, %v", one, err)
	}
}

func TestOpenStripsQueryParamsAndSqlitePrefix(t *testing.T) {
	pool, err := Open("sqlite::memory:?mode=rwc")
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = pool.Close() }()
	if err := pool.Ping(); err != nil {
		t.Fatal(err)
	}
}

func TestOpenMemoryKeepsItsDataAcrossStatements(t *testing.T) {
	pool, err := OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = pool.Close() }()
	if _, err := pool.Exec("CREATE TABLE t (v TEXT); INSERT INTO t VALUES ('kept')"); err != nil {
		t.Fatal(err)
	}
	var v string
	if err := pool.QueryRow("SELECT v FROM t").Scan(&v); err != nil || v != "kept" {
		t.Errorf("got %q, %v", v, err)
	}
}

func TestOpenFileEnablesWALPragmas(t *testing.T) {
	pool, _ := openTempFile(t)

	var mode string
	if err := pool.QueryRow("PRAGMA journal_mode").Scan(&mode); err != nil {
		t.Fatal(err)
	}
	if strings.ToLower(mode) != "wal" {
		t.Errorf("journal_mode = %q", mode)
	}
	// NORMAL == 1 (FULL == 2, OFF == 0).
	if got := pragmaInt(t, pool, "synchronous"); got != 1 {
		t.Errorf("synchronous = %d", got)
	}
	if got := pragmaInt(t, pool, "busy_timeout"); got != 5000 {
		t.Errorf("busy_timeout = %d", got)
	}
}

func TestOpenFileWithSpecialCharactersInPath(t *testing.T) {
	path := filepath.Join(t.TempDir(), "my db #1%20.sqlite3")
	pool, err := Open("sqlite:" + path)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = pool.Close() }()
	if _, err := pool.Exec("CREATE TABLE t (v TEXT)"); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(path); err != nil {
		t.Errorf("database not created at the requested path: %v", err)
	}
}

func TestCheckpointTruncatesWALFile(t *testing.T) {
	pool, path := openTempFile(t)
	if _, err := pool.Exec("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT); INSERT INTO t (v) VALUES ('a'), ('b'), ('c')"); err != nil {
		t.Fatal(err)
	}

	walPath := path + "-wal"
	before, err := os.Stat(walPath)
	if err != nil || before.Size() == 0 {
		t.Fatalf("WAL should have frames before checkpoint: %v", err)
	}

	if err := Checkpoint(context.Background(), pool); err != nil {
		t.Fatal(err)
	}

	after, err := os.Stat(walPath)
	if err != nil || after.Size() != 0 {
		t.Errorf("TRUNCATE checkpoint should zero the WAL file: %v, %v", after, err)
	}
}

func TestForeignKeysEnabledOnEveryPooledConnection(t *testing.T) {
	pool, _ := openTempFile(t)
	ctx := context.Background()

	// Hold several at once so the pool must open more than one connection.
	var conns []*sql.Conn
	for range 5 {
		conn, err := pool.Conn(ctx)
		if err != nil {
			t.Fatal(err)
		}
		defer func() { _ = conn.Close() }()
		conns = append(conns, conn)
	}
	for i, conn := range conns {
		if got := pragmaInt(t, conn, "foreign_keys"); got != 1 {
			t.Errorf("connection %d: foreign_keys = %d", i, got)
		}
	}
}

// The driver parses DATE and DATETIME columns itself; whatever it hands to a
// string scan must still go through ParseTime and ParseDate unharmed.
func TestDriverRenderingOfDateColumnsIsParseable(t *testing.T) {
	pool, err := OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = pool.Close() }()
	_, err = pool.Exec(`CREATE TABLE t (d DATE, at DATETIME);
		INSERT INTO t VALUES ('2024-01-15', '2024-01-15 10:30:00.123+00:00');
		INSERT INTO t VALUES ('2024-01-16', CURRENT_TIMESTAMP);`)
	if err != nil {
		t.Fatal(err)
	}

	rows, err := pool.Query("SELECT d, at FROM t")
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = rows.Close() }()
	for rows.Next() {
		var d, at string
		if err := rows.Scan(&d, &at); err != nil {
			t.Fatal(err)
		}
		if _, err := ParseDate(d); err != nil {
			t.Errorf("date %q: %v", d, err)
		}
		if _, err := ParseTime(at); err != nil {
			t.Errorf("time %q: %v", at, err)
		}
	}
}
