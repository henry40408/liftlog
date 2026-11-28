// Package db opens the SQLite database and holds its storage conventions.
package db

import (
	"context"
	"database/sql"
	"fmt"
	"net/url"
	"strings"

	_ "modernc.org/sqlite" // registers the "sqlite" driver
)

const (
	memoryPath       = ":memory:"
	fileMaxOpenConns = 10
)

// Open opens the database named by a `sqlite:path?query` URL. The query is
// ignored. foreign_keys is per connection, so every connection sets it.
func Open(databaseURL string) (*sql.DB, error) {
	path := databasePath(databaseURL)
	if path == memoryPath {
		return OpenMemory()
	}
	// WAL gives concurrent readers; busy_timeout absorbs lock contention
	// between the handlers' short writes. With _txlock a transaction takes the
	// write lock when it begins: a deferred one that reads and then writes fails,
	// without waiting, if another writer got in between.
	pragmas := url.Values{
		"_pragma": {
			"busy_timeout(5000)",
			"journal_mode(WAL)",
			"synchronous(NORMAL)",
			"foreign_keys(1)",
		},
		"_txlock": {"immediate"},
	}
	dsn := "file:" + (&url.URL{Path: path}).EscapedPath() + "?" + pragmas.Encode()
	return openPool(dsn, fileMaxOpenConns)
}

// OpenMemory opens a private in-memory database. It is limited to one
// connection, because every connection to :memory: would be a separate empty
// database.
func OpenMemory() (*sql.DB, error) {
	pragmas := url.Values{"_pragma": {"foreign_keys(1)"}}
	return openPool("file::memory:?"+pragmas.Encode(), 1)
}

func databasePath(databaseURL string) string {
	path := strings.TrimPrefix(databaseURL, "sqlite:")
	path, _, _ = strings.Cut(path, "?")
	return path
}

func openPool(dsn string, maxOpenConns int) (*sql.DB, error) {
	pool, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, fmt.Errorf("open database: %w", err)
	}
	pool.SetMaxOpenConns(maxOpenConns)
	if maxOpenConns == 1 {
		// A closed connection would take the in-memory database with it.
		pool.SetMaxIdleConns(1)
		pool.SetConnMaxLifetime(0)
	}
	if err := pool.Ping(); err != nil {
		_ = pool.Close()
		return nil, fmt.Errorf("open database: %w", err)
	}
	return pool, nil
}

// Checkpoint runs a truncating WAL checkpoint, so the database file is
// self-contained after shutdown.
func Checkpoint(ctx context.Context, pool *sql.DB) error {
	if _, err := pool.ExecContext(ctx, "PRAGMA wal_checkpoint(TRUNCATE)"); err != nil {
		return fmt.Errorf("wal checkpoint: %w", err)
	}
	return nil
}
