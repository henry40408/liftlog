// Package migrations embeds the SQL migrations and applies them.
package migrations

import (
	"cmp"
	"context"
	"database/sql"
	"embed"
	"errors"
	"fmt"
	"io/fs"
	"log/slog"
	"regexp"
	"slices"
	"strconv"
)

// A released file is never edited: a change to the schema is a new file.
//
//go:embed *.sql
var embeddedMigrations embed.FS

// migrationFileName is `<version>_<description>.sql`.
var migrationFileName = regexp.MustCompile(`^(\d+)_.+\.sql$`)

// baselineRecord is the row of _migrations that shows a database reached the
// baseline. Releases 0.29 to 0.31 wrote it for their last migration, and the
// baseline writes it too, so that it is still there if user_version is lost.
const baselineRecord = "013_add_user_timezone.sql"

// migration brings the schema to version, which `PRAGMA user_version` records.
type migration struct {
	version int64
	name    string
	sql     string
}

// Run brings the database to the latest schema version.
func Run(ctx context.Context, pool *sql.DB, log *slog.Logger) error {
	log.Info("Running migrations...")
	migrations, err := loadMigrations(embeddedMigrations)
	if err != nil {
		return err
	}
	if err := applyMigrations(ctx, pool, migrations, log); err != nil {
		return err
	}
	warnOnForeignKeyViolations(ctx, pool, log)

	log.Info("Migrations completed")
	return nil
}

// RunForTests is Run without logging, for test databases.
func RunForTests(ctx context.Context, pool *sql.DB) error {
	return Run(ctx, pool, slog.New(slog.DiscardHandler))
}

// loadMigrations reads every `.sql` file in the root of fsys, ordered by version.
// The first one is the baseline: it creates the whole schema of its version.
func loadMigrations(fsys fs.FS) ([]migration, error) {
	names, err := fs.Glob(fsys, "*.sql")
	if err != nil {
		return nil, fmt.Errorf("list migrations: %w", err)
	}
	if len(names) == 0 {
		return nil, errors.New("no migration files")
	}
	migrations := make([]migration, 0, len(names))
	for _, name := range names {
		m, err := readMigration(fsys, name)
		if err != nil {
			return nil, err
		}
		migrations = append(migrations, m)
	}
	slices.SortFunc(migrations, func(a, b migration) int {
		return cmp.Compare(a.version, b.version)
	})
	for i := 1; i < len(migrations); i++ {
		if migrations[i].version == migrations[i-1].version {
			return nil, fmt.Errorf("migrations share version %d", migrations[i].version)
		}
	}
	return migrations, nil
}

func readMigration(fsys fs.FS, name string) (migration, error) {
	parts := migrationFileName.FindStringSubmatch(name)
	if parts == nil {
		return migration{}, fmt.Errorf("migration file %q is not named <version>_<description>.sql", name)
	}
	// user_version is a signed 32-bit integer, and 0 means no migration yet.
	version, err := strconv.ParseInt(parts[1], 10, 32)
	if err != nil {
		return migration{}, fmt.Errorf("migration file %q: %w", name, err)
	}
	if version == 0 {
		return migration{}, fmt.Errorf("migration file %q: versions start at 1", name)
	}
	contents, err := fs.ReadFile(fsys, name)
	if err != nil {
		return migration{}, fmt.Errorf("read migration %s: %w", name, err)
	}
	return migration{version: version, name: name, sql: string(contents)}, nil
}

// versionRange holds the first and the latest schema version this build has.
type versionRange struct {
	baseline, latest int64
}

func (r versionRange) refuseNewer(current int64) error {
	if current > r.latest {
		return fmt.Errorf("database schema version %d is newer than %d, the latest this liftlog knows; run a newer liftlog",
			current, r.latest)
	}
	return nil
}

// applyMigrations applies the migrations newer than the database, one transaction
// each. migrations is ordered and starts with the baseline.
func applyMigrations(ctx context.Context, pool *sql.DB, migrations []migration, log *slog.Logger) error {
	known := versionRange{baseline: migrations[0].version, latest: migrations[len(migrations)-1].version}
	current, err := userVersion(ctx, pool)
	if err != nil {
		return err
	}
	if err := known.refuseNewer(current); err != nil {
		return err
	}
	for _, m := range migrations {
		if m.version <= current {
			continue
		}
		log.Info("Running migration", "name", m.name)
		if err := applyMigration(ctx, pool, m, known); err != nil {
			return fmt.Errorf("migration %s: %w", m.name, err)
		}
	}
	return nil
}

// applyMigration runs the script and moves user_version in one transaction, so a
// failure leaves neither half behind.
func applyMigration(ctx context.Context, pool *sql.DB, m migration, known versionRange) error {
	// The DSN's _txlock=immediate takes the write lock here, not at the first write.
	tx, err := pool.BeginTx(ctx, nil)
	if err != nil {
		return fmt.Errorf("begin: %w", err)
	}
	defer func() { _ = tx.Rollback() }()
	// Read again under the write lock: another process, maybe a newer liftlog, may
	// have got here first.
	current, err := userVersion(ctx, tx)
	if err != nil {
		return err
	}
	if err := known.refuseNewer(current); err != nil {
		return err
	}
	if current >= m.version {
		return nil
	}
	if current < known.baseline {
		// Inside the transaction, so another process cannot create tables between
		// this check and the baseline.
		if err := checkBaselineApplies(ctx, tx); err != nil {
			return err
		}
	}
	if _, err := tx.ExecContext(ctx, m.sql); err != nil {
		return fmt.Errorf("run script: %w", err)
	}
	// A PRAGMA takes no bound parameters.
	if _, err := tx.ExecContext(ctx, fmt.Sprintf("PRAGMA user_version = %d", m.version)); err != nil {
		return fmt.Errorf("set schema version: %w", err)
	}
	if err := tx.Commit(); err != nil {
		return fmt.Errorf("commit: %w", err)
	}
	return nil
}

// rowQuerier is what *sql.DB and *sql.Tx share to read one row.
type rowQuerier interface {
	QueryRowContext(ctx context.Context, query string, args ...any) *sql.Row
}

func userVersion(ctx context.Context, q rowQuerier) (int64, error) {
	var version int64
	if err := q.QueryRowContext(ctx, "PRAGMA user_version").Scan(&version); err != nil {
		return 0, fmt.Errorf("read schema version: %w", err)
	}
	return version, nil
}

// checkBaselineApplies accepts an empty database, or one that has the baseline
// record. The baseline only creates what is missing, so on an older schema it
// would leave old tables as they are.
func checkBaselineApplies(ctx context.Context, q rowQuerier) error {
	var objects, recordTables int
	err := q.QueryRowContext(ctx, `SELECT count(*), count(*) FILTER (WHERE name = '_migrations')
		FROM sqlite_master WHERE name NOT LIKE 'sqlite\_%' ESCAPE '\'`).Scan(&objects, &recordTables)
	if err != nil {
		return fmt.Errorf("inspect database: %w", err)
	}
	if objects == 0 {
		return nil
	}
	reached := false
	if recordTables > 0 {
		err := q.QueryRowContext(ctx, "SELECT EXISTS (SELECT 1 FROM _migrations WHERE name = ?)",
			baselineRecord).Scan(&reached)
		if err != nil {
			return fmt.Errorf("read old migration records: %w", err)
		}
	}
	if !reached {
		return fmt.Errorf("the database is not empty and has no record of migration %s; "+
			"if it is from an older liftlog, open it with liftlog 0.31.1 once, then upgrade", baselineRecord)
	}
	return nil
}

// warnOnForeignKeyViolations warns instead of aborting: orphans are harmless
// (session validation INNER JOINs users), a startup lockout is not.
func warnOnForeignKeyViolations(ctx context.Context, pool *sql.DB, log *slog.Logger) {
	rows, err := pool.QueryContext(ctx, "PRAGMA foreign_key_check")
	if err != nil {
		log.Warn("PRAGMA foreign_key_check failed", "error", err)
		return
	}
	defer func() { _ = rows.Close() }()

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
