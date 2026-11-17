// Package migrations embeds the SQL migrations and applies them.
package migrations

import (
	"context"
	"database/sql"
	"embed"
	"fmt"
	"log/slog"
)

//go:embed *.sql
var files embed.FS

type migration struct {
	name string
	sql  string
}

// all returns the migrations in order. File names are `NNN_description.sql`
// with a fixed-width number (gaps are fine), so name order is apply order.
func all() ([]migration, error) {
	entries, err := files.ReadDir(".")
	if err != nil {
		return nil, fmt.Errorf("read embedded migrations: %w", err)
	}

	migrations := make([]migration, 0, len(entries))
	for _, entry := range entries {
		body, err := files.ReadFile(entry.Name())
		if err != nil {
			return nil, fmt.Errorf("read migration %s: %w", entry.Name(), err)
		}
		migrations = append(migrations, migration{name: entry.Name(), sql: string(body)})
	}
	return migrations, nil
}

// Run applies the migrations not yet recorded in `_migrations`.
func Run(ctx context.Context, pool *sql.DB, log *slog.Logger) error {
	log.Info("Running migrations...")
	migrations, err := all()
	if err != nil {
		return err
	}

	// Migrations 010 and 011 switch foreign_keys off on their connection, so
	// all of it must run on one connection that is repaired before release.
	conn, err := pool.Conn(ctx)
	if err != nil {
		return fmt.Errorf("get connection: %w", err)
	}
	defer conn.Close()

	if err := applyPending(ctx, conn, migrations, log); err != nil {
		return err
	}
	if err := enableForeignKeys(ctx, conn); err != nil {
		return err
	}
	warnOnForeignKeyViolations(ctx, conn, log)

	log.Info("Migrations completed")
	return nil
}

// RunForTests applies every migration without tracking, for fresh in-memory
// test databases.
func RunForTests(ctx context.Context, pool *sql.DB) error {
	migrations, err := all()
	if err != nil {
		return err
	}
	conn, err := pool.Conn(ctx)
	if err != nil {
		return fmt.Errorf("get connection: %w", err)
	}
	defer conn.Close()

	for _, m := range migrations {
		if _, err := conn.ExecContext(ctx, m.sql); err != nil {
			return fmt.Errorf("migration %s: %w", m.name, err)
		}
	}
	return enableForeignKeys(ctx, conn)
}

func applyPending(ctx context.Context, conn *sql.Conn, migrations []migration, log *slog.Logger) error {
	_, err := conn.ExecContext(ctx, `CREATE TABLE IF NOT EXISTS _migrations (
            name TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )`)
	if err != nil {
		return fmt.Errorf("create _migrations: %w", err)
	}
	applied, err := appliedNames(ctx, conn)
	if err != nil {
		return err
	}

	for _, m := range migrations {
		if applied[m.name] {
			log.Debug("Skipping already applied migration", "name", m.name)
			continue
		}
		log.Info("Running migration", "name", m.name)
		if _, err := conn.ExecContext(ctx, m.sql); err != nil {
			return fmt.Errorf("migration %s: %w", m.name, err)
		}
		if _, err := conn.ExecContext(ctx, "INSERT INTO _migrations (name) VALUES (?)", m.name); err != nil {
			return fmt.Errorf("record migration %s: %w", m.name, err)
		}
	}
	return nil
}

func appliedNames(ctx context.Context, conn *sql.Conn) (map[string]bool, error) {
	rows, err := conn.QueryContext(ctx, "SELECT name FROM _migrations")
	if err != nil {
		return nil, fmt.Errorf("list applied migrations: %w", err)
	}
	defer rows.Close()

	applied := map[string]bool{}
	for rows.Next() {
		var name string
		if err := rows.Scan(&name); err != nil {
			return nil, fmt.Errorf("scan applied migration: %w", err)
		}
		applied[name] = true
	}
	return applied, rows.Err()
}

func enableForeignKeys(ctx context.Context, conn *sql.Conn) error {
	if _, err := conn.ExecContext(ctx, "PRAGMA foreign_keys=ON"); err != nil {
		return fmt.Errorf("enable foreign keys: %w", err)
	}
	return nil
}

// warnOnForeignKeyViolations warns instead of aborting: orphans are harmless
// (session validation INNER JOINs users), a startup lockout is not.
func warnOnForeignKeyViolations(ctx context.Context, conn *sql.Conn, log *slog.Logger) {
	rows, err := conn.QueryContext(ctx, "PRAGMA foreign_key_check")
	if err != nil {
		log.Warn("PRAGMA foreign_key_check failed", "error", err)
		return
	}
	defer rows.Close()

	count := 0
	for rows.Next() {
		count++
	}
	if err := rows.Err(); err != nil {
		log.Warn("PRAGMA foreign_key_check failed", "error", err)
		return
	}
	if count > 0 {
		log.Warn("PRAGMA foreign_key_check reported violations after migrations", "count", count)
	}
}
